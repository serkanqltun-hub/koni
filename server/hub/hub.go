package hub

import (
	"crypto/rand"
	"encoding/base64"
	"errors"
	"log"
	"sync"
	"time"

	"github.com/serkanqltun-hub/koni/server/store"
)

// Hub is the room registry.
type Hub struct {
	cfg     Config
	mu      sync.Mutex
	byID    map[string]*Room
	byToken map[string]*Room
	limit   limiter // login/register attempts per IP
	reports limiter // bug reports per IP
	soc     social  // lobby connections, presence, room calls (social.go)
}

func New(cfg Config) *Hub {
	return &Hub{
		cfg: cfg.withDefaults(), byID: map[string]*Room{}, byToken: map[string]*Room{},
		limit:   limiter{n: authAttempts, window: authWindow},
		reports: limiter{n: reportsPerHour, window: time.Hour},
		soc:     social{conns: map[int64]map[*lobbyConn]struct{}{}, calls: map[int64][]roomCall{}, poke: make(chan struct{}, 1)},
	}
}

func token(n int) string {
	b := make([]byte, n)
	if _, err := rand.Read(b); err != nil {
		panic(err)
	}
	return base64.RawURLEncoding.EncodeToString(b)
}

func (h *Hub) remove(r *Room) {
	h.mu.Lock()
	delete(h.byID, r.id)
	delete(h.byToken, r.invite)
	h.mu.Unlock()
}

// RoomCount is for tests and /healthz diagnostics.
func (h *Hub) RoomCount() int {
	h.mu.Lock()
	defer h.mu.Unlock()
	return len(h.byID)
}

// Session ties one connection to its member.
type Session struct {
	Room    *Room
	Member  *Member
	Snap    Snapshot
	Invite  string
	RoomID  string
	Token   string // resume secret
	Resumed bool
	out     chan []byte
}

// Out is this connection's outbound queue; it is closed when the connection is
// superseded or the member leaves.
func (s *Session) Out() <-chan []byte { return s.out }

// Handle forwards a client message to the room actor.
func (s *Session) Handle(in Inbound) {
	s.Room.do(func() { s.Room.handle(s.Member, in) })
}

// Close ends this connection; the member becomes a ghost that may resume.
func (s *Session) Close() {
	s.Room.do(func() { s.Room.disconnect(s.Member, s.out) })
}

// CreateRoom makes a room and joins the creator as host. errCode is "" on success.
// auth is the account session token (ignored on an open server).
func (h *Hub) CreateRoom(name, username, policy, auth string) (*Session, string) {
	if policy == "" {
		policy = PolicyFree
	}
	if policy != PolicyFree && policy != PolicyHostOnly {
		return nil, "bad_message"
	}
	username, user, code := h.identify(username, auth)
	if code != "" {
		return nil, code
	}
	if _, ok := validName(username); !ok {
		return nil, "bad_username"
	}
	if n := len([]rune(name)); n == 0 || n > 48 {
		name = "Oda"
	}
	id, inv := "r-"+token(6), token(12)
	r := newRoom(id, name, inv, policy, h.cfg, h.remove)
	h.mu.Lock()
	h.byID[id] = r
	h.byToken[inv] = r
	h.mu.Unlock()
	return h.joinRoom(r, username, user, true)
}

// JoinRoom joins by invite token.
func (h *Hub) JoinRoom(invite, username, auth string) (*Session, string) {
	h.mu.Lock()
	r := h.byToken[invite]
	h.mu.Unlock()
	if r == nil {
		return nil, "bad_invite"
	}
	username, user, code := h.identify(username, auth)
	if code != "" {
		return nil, code
	}
	if !h.mayJoin(r, user) {
		return nil, "not_invited"
	}
	return h.joinRoom(r, username, user, false)
}

// identify resolves the member name: the account's on a server with a store, else
// the chosen username.
func (h *Hub) identify(username, auth string) (string, int64, string) {
	if h.cfg.Store == nil {
		return username, 0, ""
	}
	if auth == "" {
		return "", 0, "auth_required"
	}
	u, err := h.cfg.Store.UserByToken(auth)
	if err != nil {
		return "", 0, errCode(err)
	}
	return u.Name, u.ID, ""
}

// errCode maps a store failure to a protocol error code.
func errCode(err error) string {
	var e store.Error
	if errors.As(err, &e) {
		return e.Code()
	}
	log.Printf("store: %v", err)
	return "server_error"
}

// Resume reattaches a dropped member (room id + member id + resume token).
func (h *Hub) Resume(roomID, memberID, tok string) (*Session, string) {
	h.mu.Lock()
	r := h.byID[roomID]
	h.mu.Unlock()
	if r == nil {
		return nil, "resume_failed"
	}
	var res joinResult
	if !r.call(func() { res = r.resume(memberID, tok) }) {
		return nil, "resume_failed"
	}
	if res.err != "" {
		return nil, res.err
	}
	s := r.session(res)
	s.Resumed = true
	return s, ""
}

func (h *Hub) joinRoom(r *Room, username string, user int64, host bool) (*Session, string) {
	var res joinResult
	if !r.call(func() { res = r.join(username, user, host) }) {
		return nil, "bad_invite"
	}
	if res.err != "" {
		if host { // creator failed to join its own fresh room
			r.do(r.shutdown)
		}
		return nil, res.err
	}
	return r.session(res), ""
}

func (r *Room) session(res joinResult) *Session {
	return &Session{Room: r, Member: res.m, Snap: res.snap, Invite: r.invite, RoomID: r.id, Token: res.token, out: res.out}
}
