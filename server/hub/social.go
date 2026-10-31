package hub

// Profiles, avatars, presence and room calls (PROTOCOL.md §3.0d–f, p0.18):
//   - `profile` handshake: own name#tag, picture, invite codes, last 7 days.
//   - pictures come only from the server's avatars directory (GET /avatars/{id},
//     session token required); members pick one of those, nothing is uploaded.
//   - `hello`: a long-lived lobby connection; the server pushes `presence` (friends,
//     who is online and in which room, pending requests and calls).
//   - in a room, `call` asks a mutual friend in; joining a room needs a call or a
//     mutual friend inside.

import (
	"cmp"
	"context"
	"encoding/json"
	"maps"
	"math"
	"net/http"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"time"

	"github.com/coder/websocket"
	"github.com/serkanqltun-hub/koni/server/store"
)

const (
	presenceEvery = 3 * time.Second
	callTTL       = 30 * time.Minute
	inviteTTL     = 7 * 24 * time.Hour
)

var avatarExt = []string{".jpg", ".jpeg", ".png", ".webp"}

// avatars lists picture ids (file names without extension), sorted. Read on every
// call so pictures can be added on the server without a restart.
func (h *Hub) avatars() []string {
	if h.cfg.Avatars == "" {
		return nil
	}
	ents, err := os.ReadDir(h.cfg.Avatars)
	if err != nil {
		return nil
	}
	var out []string
	for _, e := range ents {
		ext := strings.ToLower(filepath.Ext(e.Name()))
		if e.Type().IsRegular() && slices.Contains(avatarExt, ext) {
			out = append(out, strings.TrimSuffix(e.Name(), filepath.Ext(e.Name())))
		}
	}
	slices.Sort(out)
	return slices.Compact(out)
}

func (h *Hub) validAvatar(id string) bool {
	return id == "" || slices.Contains(h.avatars(), id)
}

// serveAvatar: GET /avatars/{id} with "Authorization: Bearer <session>". Pictures are
// shown only to members (they are not ours to publish).
func (h *Hub) serveAvatar(w http.ResponseWriter, req *http.Request) {
	st := h.cfg.Store
	tok := strings.TrimPrefix(req.Header.Get("Authorization"), "Bearer ")
	if st == nil || tok == "" {
		http.Error(w, "auth", http.StatusUnauthorized)
		return
	}
	if _, err := st.UserByToken(tok); err != nil {
		http.Error(w, "auth", http.StatusUnauthorized)
		return
	}
	id := req.PathValue("id")
	if id == "" || !slices.Contains(h.avatars(), id) {
		http.NotFound(w, req)
		return
	}
	for _, ext := range avatarExt {
		p := filepath.Join(h.cfg.Avatars, id+ext)
		if _, err := os.Stat(p); err == nil {
			w.Header().Set("Cache-Control", "private, max-age=86400")
			http.ServeFile(w, req, p)
			return
		}
	}
	http.NotFound(w, req)
}

// serveAuth: GET /auth with "X-Koni-Token: <session>" → 204 or 401. nginx asks it
// (auth_request) so that only members can use the optional provider API relay. The
// header is separate because that request already carries the member's own key.
func (h *Hub) serveAuth(w http.ResponseWriter, req *http.Request) {
	st := h.cfg.Store
	tok := req.Header.Get("X-Koni-Token")
	if st == nil || tok == "" {
		http.Error(w, "auth", http.StatusUnauthorized)
		return
	}
	if _, err := st.UserByToken(tok); err != nil {
		http.Error(w, "auth", http.StatusUnauthorized)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// Profile answers a `profile` handshake. Op "avatar" sets the picture, "invite" makes
// a one-time account code (its user becomes the caller's friend), "" just reads.
func (h *Hub) Profile(in Inbound) map[string]any {
	st := h.cfg.Store
	if st == nil {
		return errMsg("no_accounts")
	}
	if in.Auth == "" {
		return errMsg("auth_required")
	}
	u, err := st.UserByToken(in.Auth)
	if err != nil {
		return errMsg(errCode(err))
	}
	out := map[string]any{"t": "profile"}
	switch in.Op {
	case "", "get":
	case "avatar":
		if in.Avatar == "" || !h.validAvatar(in.Avatar) {
			return errMsg("bad_avatar")
		}
		if slices.Contains(h.takenAvatars(u.ID), in.Avatar) {
			return errMsg("avatar_taken")
		}
		if err := st.SetAvatar(u.ID, in.Avatar); err != nil {
			return errMsg(errCode(err))
		}
		u.Avatar = in.Avatar
		h.pokePresence()
	case "upload":
		if !u.Admin {
			return errMsg("forbidden")
		}
		id, code := h.uploadAvatar(in.Image)
		if code != "" {
			return errMsg(code)
		}
		if err := st.SetAvatar(u.ID, id); err != nil {
			return errMsg(errCode(err))
		}
		u.Avatar = id
		h.pokePresence()
	case "invite":
		code, err := st.CreateInvite(u.ID, inviteTTL)
		if err != nil {
			return errMsg(errCode(err))
		}
		out["code"] = code
	default:
		return errMsg("bad_message")
	}
	invites, err := st.Invites(u.ID)
	if err != nil {
		return errMsg(errCode(err))
	}
	days, err := st.Daily(u.ID, h.cfg.Now())
	if err != nil {
		return errMsg(errCode(err))
	}
	out["name"], out["tag"], out["avatar"], out["admin"] = u.Name, u.Tag, u.Avatar, u.Admin
	out["canInvite"] = u.CanInvite || u.Admin
	out["taken"] = h.takenAvatars(u.ID)
	out["avatars"], out["invites"], out["daily"] = h.avatars(), invites, days
	return out
}

// ---------------------------------------------------------------- presence + calls

type lobbyConn struct {
	user int64
	out  chan []byte
	last string // last presence sent, to skip unchanged pushes
}

type roomCall struct {
	room   *Room
	From   string `json:"from"`
	Avatar string `json:"avatar"`
	Name   string `json:"room"`
	Invite string `json:"invite"`
	At     int64  `json:"at"`
}

type social struct {
	mu    sync.Mutex
	conns map[int64]map[*lobbyConn]struct{}
	// phone remotes (remote.go), per account
	remotes map[int64]map[*lobbyConn]struct{}
	calls   map[int64][]roomCall // target account → pending calls
	poke    chan struct{}
	start   sync.Once
}

func (h *Hub) pokePresence() {
	select {
	case h.soc.poke <- struct{}{}:
	default:
	}
}

// where: account → its room right now (name, media title, invite).
type whereabouts struct {
	Room   string
	Title  string
	Invite string
}

// roomCard is one open room on the home screen ("Açık odalar", p0.22).
type roomCard struct {
	Invite  string   `json:"invite"`
	Room    string   `json:"room"`
	Title   string   `json:"title,omitempty"`
	View    string   `json:"view"`
	Night   bool     `json:"night,omitempty"`   // "Ne izlesek?" is running
	Playing bool     `json:"playing,omitempty"` // media is playing right now
	Pos     float64  `json:"pos,omitempty"`
	Dur     float64  `json:"dur,omitempty"`
	Members []string `json:"members"`        // Ad#1234, join order
	Left    int64    `json:"left,omitempty"` // you were in it: when you left (unix ms)
	Mine    bool     `json:"mine,omitempty"` // you were in it, so you may come back

	users  []int64         // accounts inside
	alumni map[int64]int64 // accounts ever inside → left at
}

func (h *Hub) whereabouts() (map[int64]whereabouts, []roomCard) {
	h.mu.Lock()
	rooms := make([]*Room, 0, len(h.byID))
	for _, r := range h.byID {
		rooms = append(rooms, r)
	}
	h.mu.Unlock()
	out := map[int64]whereabouts{}
	var cards []roomCard
	for _, r := range rooms {
		r.call(func() {
			c := roomCard{Invite: r.invite, Room: r.name, View: r.view, Night: r.night != nil, Members: []string{}, alumni: maps.Clone(r.alumni)}
			if r.media != nil {
				c.Title, c.Dur = r.media.Title, r.media.Duration
				c.Playing = r.mode == ModePlaying
				c.Pos = math.Floor(r.position(r.nowMs())/30) * 30 // coarse: presence is pushed only on change
			}
			for _, id := range r.order {
				m := r.members[id]
				if m == nil || m.closed {
					continue
				}
				c.Members = append(c.Members, m.Name)
				if m.User != 0 && !m.ghost {
					out[m.User] = whereabouts{Room: r.name, Title: c.Title, Invite: r.invite}
					c.users = append(c.users, m.User)
				}
			}
			cards = append(cards, c)
		})
	}
	return out, cards
}

// roomUsers: account ids currently in r.
func roomUsers(r *Room) []int64 {
	var ids []int64
	r.call(func() {
		for _, m := range r.members {
			if m.User != 0 && !m.closed {
				ids = append(ids, m.User)
			}
		}
	})
	return ids
}

// presenceLoop pushes `presence` to every lobby connection whose view changed.
func (h *Hub) presenceLoop() {
	t := time.NewTicker(presenceEvery)
	defer t.Stop()
	for {
		select {
		case <-t.C:
		case <-h.soc.poke:
		}
		h.pushPresence()
	}
}

type presenceFriend struct {
	Name   string `json:"name"`
	Avatar string `json:"avatar"`
	Mutual bool   `json:"mutual"`
	Admin  bool   `json:"admin,omitempty"`
	Online bool   `json:"online"`
	Room   string `json:"room,omitempty"`
	Title  string `json:"title,omitempty"`
	Invite string `json:"invite,omitempty"` // p0.22: their room, to join from home
}

func (h *Hub) pushPresence() {
	st := h.cfg.Store
	where, cards := h.whereabouts()
	now := h.cfg.Now()
	h.soc.mu.Lock()
	online := map[int64]bool{}
	var conns []*lobbyConn
	for u, set := range h.soc.conns {
		online[u] = true
		for c := range set {
			conns = append(conns, c)
		}
	}
	for u, calls := range h.soc.calls { // drop old calls and calls to closed rooms
		calls = slices.DeleteFunc(calls, func(c roomCall) bool {
			return now.UnixMilli()-c.At > callTTL.Milliseconds() || c.room.closedNow()
		})
		if len(calls) == 0 {
			delete(h.soc.calls, u)
		} else {
			h.soc.calls[u] = calls
		}
	}
	callsOf := map[int64][]roomCall{}
	for u, c := range h.soc.calls {
		callsOf[u] = slices.Clone(c)
	}
	h.soc.mu.Unlock()
	for u := range where {
		online[u] = true
	}
	cache := map[int64]store.FriendList{}
	for _, c := range conns {
		list, ok := cache[c.user]
		if !ok {
			var err error
			if list, err = st.Friends(c.user); err != nil {
				continue
			}
			cache[c.user] = list
		}
		friends := make([]presenceFriend, 0, len(list.Friends))
		for _, f := range list.Friends {
			p := presenceFriend{Name: f.Name, Avatar: f.Avatar, Mutual: f.Mutual, Admin: f.Admin}
			if f.Mutual { // only people who added you back see where you are
				p.Online = online[f.ID]
				p.Room, p.Title, p.Invite = where[f.ID].Room, where[f.ID].Title, where[f.ID].Invite
			}
			friends = append(friends, p)
		}
		rooms := openRooms(c.user, list, cards)
		calls := callsOf[c.user]
		if calls == nil {
			calls = []roomCall{}
		}
		b := enc(map[string]any{"t": "presence", "friends": friends, "requests": list.Requests, "calls": calls, "rooms": rooms})
		if string(b) == c.last {
			continue
		}
		c.last = string(b)
		select {
		case c.out <- b:
		default:
		}
	}
}

// openRooms: the rooms user may enter from home: ones they were in (theirs to come
// back to) and ones a mutual friend is in now. Theirs first, then fuller rooms.
func openRooms(user int64, list store.FriendList, cards []roomCard) []roomCard {
	mutual := map[int64]bool{}
	for _, f := range list.Friends {
		if f.Mutual {
			mutual[f.ID] = true
		}
	}
	out := []roomCard{}
	for _, c := range cards {
		left, was := c.alumni[user]
		if was && left == 0 {
			continue // inside right now
		}
		if !was && !slices.ContainsFunc(c.users, func(u int64) bool { return mutual[u] }) {
			continue
		}
		if was {
			c.Mine, c.Left = true, left
		}
		out = append(out, c)
	}
	slices.SortStableFunc(out, func(a, b roomCard) int {
		if a.Mine != b.Mine {
			if a.Mine {
				return -1
			}
			return 1
		}
		if a.Mine && a.Left != b.Left {
			return cmp.Compare(b.Left, a.Left)
		}
		return cmp.Compare(len(b.Members), len(a.Members))
	})
	if len(out) > 6 {
		out = out[:6]
	}
	return out
}

// Lobby serves a `hello` connection until it drops: presence pushes out, pings in.
func (h *Hub) Lobby(ctx context.Context, c *websocket.Conn, in Inbound) {
	st := h.cfg.Store
	if st == nil {
		writeJSON(ctx, c, errMsg("no_accounts"))
		return
	}
	u, err := st.UserByToken(in.Auth)
	if err != nil {
		writeJSON(ctx, c, errMsg(errCode(err)))
		return
	}
	h.soc.start.Do(func() { go h.presenceLoop() })
	lc := &lobbyConn{user: u.ID, out: make(chan []byte, 16)}
	h.soc.mu.Lock()
	if h.soc.conns[u.ID] == nil {
		h.soc.conns[u.ID] = map[*lobbyConn]struct{}{}
	}
	h.soc.conns[u.ID][lc] = struct{}{}
	h.soc.mu.Unlock()
	defer func() {
		h.soc.mu.Lock()
		delete(h.soc.conns[u.ID], lc)
		if len(h.soc.conns[u.ID]) == 0 {
			delete(h.soc.conns, u.ID)
		}
		h.soc.mu.Unlock()
		h.pokePresence()
	}()
	writeJSON(ctx, c, map[string]any{"t": "hello", "protocol": ProtocolVersion, "name": u.Name, "tag": u.Tag, "avatar": u.Avatar, "admin": u.Admin})
	h.pokePresence()

	wctx, cancel := context.WithCancel(ctx)
	defer cancel()
	go func() {
		defer cancel()
		for {
			select {
			case b := <-lc.out:
				if writeJSON(wctx, c, json.RawMessage(b)) != nil {
					return
				}
			case <-wctx.Done():
				return
			}
		}
	}()
	for {
		rctx, rcancel := context.WithTimeout(wctx, readIdle)
		_, data, err := c.Read(rctx)
		rcancel()
		if err != nil {
			return
		}
		var m Inbound
		if json.Unmarshal(data, &m) == nil && m.T == "decline" {
			h.dropCall(u.ID, m.Invite)
			h.pokePresence()
		} else if m.T == "remote_state" {
			h.remoteReport(u.ID, data)
		}
	}
}

// Call asks a mutual friend into the caller's room; the call shows on their home screen.
func (h *Hub) Call(s *Session, handle string) map[string]any {
	st := h.cfg.Store
	if st == nil || s.Member.User == 0 {
		return errMsg("no_accounts")
	}
	me, err := st.UserByID(s.Member.User)
	if err != nil {
		return errMsg(errCode(err))
	}
	target, err := st.Lookup(handle)
	if err != nil {
		return errMsg(errCode(err))
	}
	if ok, err := st.Mutual(me.ID, target.ID); err != nil || !ok {
		return errMsg("not_friends")
	}
	c := roomCall{room: s.Room, From: me.Handle(), Avatar: me.Avatar, Name: s.Room.name, Invite: s.Room.invite, At: h.cfg.Now().UnixMilli()}
	h.soc.mu.Lock()
	calls := slices.DeleteFunc(h.soc.calls[target.ID], func(x roomCall) bool { return x.room == s.Room })
	h.soc.calls[target.ID] = append(calls, c)
	h.soc.mu.Unlock()
	h.soc.start.Do(func() { go h.presenceLoop() })
	h.pokePresence()
	return map[string]any{"t": "called", "user": target.Handle()}
}

func (h *Hub) dropCall(user int64, invite string) {
	h.soc.mu.Lock()
	defer h.soc.mu.Unlock()
	h.soc.calls[user] = slices.DeleteFunc(h.soc.calls[user], func(c roomCall) bool { return c.Invite == invite })
}

// mayJoin: on an account server a room is entered with a call, as a mutual friend of
// someone inside, or when it is empty (reopening from "Son odalar").
func (h *Hub) mayJoin(r *Room, user int64) bool {
	st := h.cfg.Store
	if st == nil || user == 0 {
		return true
	}
	h.soc.mu.Lock()
	called := slices.ContainsFunc(h.soc.calls[user], func(c roomCall) bool { return c.room == r })
	h.soc.mu.Unlock()
	if called {
		h.dropCall(user, r.invite)
		return true
	}
	inside := roomUsers(r)
	if len(inside) == 0 || slices.Contains(inside, user) {
		return true
	}
	back := false // was in this room before: may come back while it is open (p0.22)
	r.call(func() { _, back = r.alumni[user] })
	if back {
		return true
	}
	for _, id := range inside {
		if ok, _ := st.Mutual(user, id); ok {
			return true
		}
	}
	return false
}
