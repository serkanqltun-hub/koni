package hub

import (
	"context"
	_ "embed"
	"encoding/json"
	"math"
	"net/http"
	"strings"

	"github.com/coder/websocket"
)

// Phone remote (PROTOCOL.md §3.0h, p0.24): a small web page signs in with the
// account and sends play/pause, seek and volume to the same account's desktop app
// over its lobby connection. The desktop acts as if its own button was pressed, so
// a room stays in sync.

//go:embed remote.html
var remotePage []byte

// serveRemotePage answers a plain browser GET on /ws (no WebSocket upgrade), so the
// page needs no extra route in front of the server.
func serveRemotePage(w http.ResponseWriter) {
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("X-Frame-Options", "DENY")
	w.Write(remotePage)
}

func isUpgrade(req *http.Request) bool {
	return strings.EqualFold(req.Header.Get("Upgrade"), "websocket")
}

type remoteMsg struct {
	T  string  `json:"t"`
	Op string  `json:"op"`
	By float64 `json:"by"`
}

// remoteState is what the desktop reports back; the server passes it on to phones.
type remoteState struct {
	Title  string   `json:"title"`
	Paused bool     `json:"paused"`
	Pos    *float64 `json:"pos,omitempty"`
	Dur    *float64 `json:"dur,omitempty"`
	Vol    int      `json:"vol"`
}

var remoteOps = map[string]float64{"toggle": 0, "seek": 600, "vol": 100, "status": 0}

// Remote serves a phone's `remote` connection until it drops.
func (h *Hub) Remote(ctx context.Context, c *websocket.Conn, in Inbound) {
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
	rc := &lobbyConn{user: u.ID, out: make(chan []byte, 16)}
	h.soc.mu.Lock()
	if h.soc.remotes == nil {
		h.soc.remotes = map[int64]map[*lobbyConn]struct{}{}
	}
	if h.soc.remotes[u.ID] == nil {
		h.soc.remotes[u.ID] = map[*lobbyConn]struct{}{}
	}
	h.soc.remotes[u.ID][rc] = struct{}{}
	h.soc.mu.Unlock()
	defer func() {
		h.soc.mu.Lock()
		delete(h.soc.remotes[u.ID], rc)
		if len(h.soc.remotes[u.ID]) == 0 {
			delete(h.soc.remotes, u.ID)
		}
		h.soc.mu.Unlock()
	}()
	writeJSON(ctx, c, map[string]any{"t": "remote", "protocol": ProtocolVersion, "name": u.Handle()})

	wctx, cancel := context.WithCancel(ctx)
	defer cancel()
	go func() {
		defer cancel()
		for {
			select {
			case b := <-rc.out:
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
		var m remoteMsg
		if json.Unmarshal(data, &m) != nil || m.T != "remote" {
			continue
		}
		limit, ok := remoteOps[m.Op]
		if !ok || math.IsNaN(m.By) {
			continue
		}
		m.By = math.Max(-limit, math.Min(limit, m.By))
		if !h.toDesktops(u.ID, enc(m)) {
			push(rc, enc(map[string]any{"t": "remote_state", "online": false}))
		}
	}
}

// toDesktops sends b to every lobby (desktop app) of user; false if none is online.
func (h *Hub) toDesktops(user int64, b []byte) bool {
	h.soc.mu.Lock()
	defer h.soc.mu.Unlock()
	for lc := range h.soc.conns[user] {
		push(lc, b)
	}
	return len(h.soc.conns[user]) > 0
}

// remoteReport passes a desktop's `remote_state` on to the user's phones.
func (h *Hub) remoteReport(user int64, data []byte) {
	var s remoteState
	if json.Unmarshal(data, &s) != nil {
		return
	}
	if r := []rune(s.Title); len(r) > 200 {
		s.Title = string(r[:200])
	}
	b := enc(map[string]any{"t": "remote_state", "online": true, "title": s.Title, "paused": s.Paused, "pos": s.Pos, "dur": s.Dur, "vol": s.Vol})
	h.soc.mu.Lock()
	defer h.soc.mu.Unlock()
	for rc := range h.soc.remotes[user] {
		push(rc, b)
	}
}

func push(c *lobbyConn, b []byte) {
	select {
	case c.out <- b:
	default:
	}
}
