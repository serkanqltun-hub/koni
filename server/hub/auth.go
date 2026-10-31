package hub

import (
	"net"
	"net/http"
	"slices"
	"sync"
	"time"

	"github.com/serkanqltun-hub/koni/server/store"
)

const (
	authAttempts = 10               // login/register attempts per IP …
	authWindow   = 10 * time.Minute // … in this window
)

// limiter allows n attempts per IP per window, in memory only (docs/design/accounts.md §5.1).
type limiter struct {
	n      int
	window time.Duration
	mu     sync.Mutex
	hits   map[string][]time.Time
}

func (l *limiter) allow(ip string, now time.Time) bool {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.hits == nil {
		l.hits = map[string][]time.Time{}
	}
	recent := l.hits[ip][:0]
	for _, t := range l.hits[ip] {
		if now.Sub(t) < l.window {
			recent = append(recent, t)
		}
	}
	if len(recent) >= l.n {
		l.hits[ip] = recent
		return false
	}
	l.hits[ip] = append(recent, now)
	if len(l.hits) > 10000 { // bounded: forget everyone rather than grow
		l.hits = map[string][]time.Time{}
	}
	return true
}

// clientIP: behind the local nginx the real address is in X-Real-IP.
func clientIP(req *http.Request) string {
	host, _, err := net.SplitHostPort(req.RemoteAddr)
	if err != nil {
		host = req.RemoteAddr
	}
	if ip := net.ParseIP(host); ip != nil && ip.IsLoopback() {
		if real := req.Header.Get("X-Real-IP"); real != "" {
			return real
		}
	}
	return host
}

// Auth answers a register/login/logout handshake (PROTOCOL.md §3.0). The reply is
// an `auth` message on success, otherwise an `error`.
func (h *Hub) Auth(in Inbound, ip string) map[string]any {
	st := h.cfg.Store
	if st == nil {
		return errMsg("no_accounts")
	}
	if in.T == "logout" {
		if err := st.Logout(in.Auth); err != nil {
			return errMsg(errCode(err))
		}
		return map[string]any{"t": "auth", "token": ""}
	}
	if !h.limit.allow(ip, h.cfg.Now()) {
		return errMsg("rate_limited")
	}
	var (
		user store.User
		tok  string
		err  error
	)
	if in.T == "register" {
		if !h.validAvatar(in.Avatar) || slices.Contains(h.takenAvatars(0), in.Avatar) {
			return errMsg("bad_avatar")
		}
		user, tok, err = st.Register(in.Code, in.Username, in.Password, in.Device, in.Avatar)
	} else {
		user, tok, err = st.Login(in.Username, in.Password, in.Device)
	}
	if err != nil {
		code := errCode(err)
		if code == "bad_username" { // account names are stricter than room names
			return map[string]any{"t": "error", "code": code, "message": "Kullanıcı adı 2–24 karakter olmalı"}
		}
		return errMsg(code)
	}
	return map[string]any{"t": "auth", "token": tok, "name": user.Name, "tag": user.Tag, "avatar": user.Avatar}
}

// Stats answers a `stats` handshake (PROTOCOL.md §3.0a): the caller's and their mutual
// friends' totals for the period plus badges (docs/design/accounts.md §5.4).
func (h *Hub) Stats(in Inbound) map[string]any {
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
	since, ok := store.PeriodStart(in.Period, h.cfg.Now())
	if !ok {
		return errMsg("bad_period")
	}
	users, err := st.Stats(u.ID, since)
	if err != nil {
		return errMsg("server_error")
	}
	return map[string]any{"t": "stats", "period": in.Period, "you": u.Handle(), "users": users, "badges": store.Badges(users)}
}

// Feed answers a `feed` handshake (p0.22): the last week of you and your mutual
// friends, for the home screen's "Etkinlik" tab.
func (h *Hub) Feed(in Inbound) map[string]any {
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
	items, err := st.Feed(u.ID, h.cfg.Now().Add(-7*24*time.Hour).UnixMilli(), 30)
	if err != nil {
		return errMsg("server_error")
	}
	if items == nil {
		items = []store.FeedItem{}
	}
	return map[string]any{"t": "feed", "you": u.Handle(), "items": items}
}

// Friends answers a `friends` handshake (PROTOCOL.md §3.0c): op "add"/"remove"/"decline" changes the
// caller's list (User is the account name), then every op replies with the list.
func (h *Hub) Friends(in Inbound) map[string]any {
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
	switch in.Op {
	case "", "list":
	case "add":
		err = st.AddFriend(u.ID, in.User)
	case "remove":
		err = st.RemoveFriend(u.ID, in.User)
	case "decline":
		err = st.DeclineFriend(u.ID, in.User)
	default:
		return errMsg("bad_message")
	}
	if err != nil {
		return errMsg(errCode(err))
	}
	list, err := st.Friends(u.ID)
	if err != nil {
		return errMsg(errCode(err))
	}
	return map[string]any{"t": "friends", "friends": list.Friends, "requests": list.Requests}
}

func errMsg(code string) map[string]any {
	return map[string]any{"t": "error", "code": code, "message": errText[code]}
}
