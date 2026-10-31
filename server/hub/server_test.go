package hub_test

import (
	"context"
	"encoding/json"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/coder/websocket"
	"github.com/serkanqltun-hub/koni/server/hub"
)

const readTimeout = 2 * time.Second

type msg map[string]any

func (m msg) s(k string) string  { s, _ := m[k].(string); return s }
func (m msg) f(k string) float64 { f, _ := m[k].(float64); return f }

type client struct {
	t    *testing.T
	conn *websocket.Conn
}

func newServer(t *testing.T) (*hub.Hub, *httptest.Server) {
	t.Helper()
	h := hub.New(hub.Config{MinLead: 30 * time.Millisecond, GhostWait: 50 * time.Millisecond, EmptyWait: 100 * time.Millisecond})
	srv := httptest.NewServer(h.Handler())
	t.Cleanup(srv.Close)
	return h, srv
}

func dial(t *testing.T, srv *httptest.Server) *client {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), readTimeout)
	defer cancel()
	url := "ws" + strings.TrimPrefix(srv.URL, "http") + "/ws"
	c, _, err := websocket.Dial(ctx, url, nil)
	if err != nil {
		t.Fatalf("dial: %v", err)
	}
	t.Cleanup(func() { c.CloseNow() })
	return &client{t: t, conn: c}
}

func (c *client) send(v any) {
	c.t.Helper()
	b, _ := json.Marshal(v)
	ctx, cancel := context.WithTimeout(context.Background(), readTimeout)
	defer cancel()
	if err := c.conn.Write(ctx, websocket.MessageText, b); err != nil {
		c.t.Fatalf("write: %v", err)
	}
}

// next reads one message or fails after the timeout.
func (c *client) next() msg {
	c.t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), readTimeout)
	defer cancel()
	_, data, err := c.conn.Read(ctx)
	if err != nil {
		c.t.Fatalf("read: %v", err)
	}
	var m msg
	if err := json.Unmarshal(data, &m); err != nil {
		c.t.Fatalf("bad json %q: %v", data, err)
	}
	return m
}

// wait skips messages until pred matches.
func (c *client) wait(desc string, pred func(msg) bool) msg {
	c.t.Helper()
	for {
		ctx, cancel := context.WithTimeout(context.Background(), readTimeout)
		_, data, err := c.conn.Read(ctx)
		cancel()
		if err != nil {
			c.t.Fatalf("waiting for %s: %v", desc, err)
		}
		var m msg
		if json.Unmarshal(data, &m) != nil {
			c.t.Fatalf("bad json %q", data)
		}
		if pred(m) {
			return m
		}
	}
}

func (c *client) waitType(typ string) msg {
	c.t.Helper()
	return c.wait(typ, func(m msg) bool { return m.s("t") == typ })
}

func (c *client) waitState(mode, cause string) msg {
	c.t.Helper()
	return c.wait("state "+mode+"/"+cause, func(m msg) bool {
		return m.s("t") == "state" && m.s("mode") == mode && (cause == "" || m.s("cause") == cause)
	})
}

func (c *client) waitDenied(reason string) msg {
	c.t.Helper()
	return c.wait("denied "+reason, func(m msg) bool { return m.s("t") == "denied" && m.s("reason") == reason })
}

// createRoom returns the client, its member id and the invite.
func createRoom(t *testing.T, srv *httptest.Server, user, policy string) (*client, string, string) {
	t.Helper()
	c := dial(t, srv)
	c.send(msg{"t": "create_room", "name": "Oda", "username": user, "policy": policy})
	rc := c.next()
	if rc.s("t") != "room_created" || rc.s("invite") == "" || rc.s("you") == "" {
		t.Fatalf("expected room_created, got %v", rc)
	}
	j := c.next()
	if j.s("t") != "joined" || j.s("invite") != rc.s("invite") || j.s("you") != rc.s("you") {
		t.Fatalf("expected joined, got %v", j)
	}
	return c, rc.s("you"), rc.s("invite")
}

func joinRoom(t *testing.T, srv *httptest.Server, invite, user string) (*client, string) {
	t.Helper()
	c := dial(t, srv)
	c.send(msg{"t": "join", "invite": invite, "username": user})
	j := c.next()
	if j.s("t") != "joined" {
		t.Fatalf("expected joined, got %v", j)
	}
	return c, j.s("you")
}

// setup creates a room with host A and guest B.
func setup(t *testing.T, policy string) (h *hub.Hub, a, b *client, aid, bid string) {
	t.Helper()
	h, srv := newServer(t)
	a, aid, inv := createRoom(t, srv, "Alice", policy)
	b, bid = joinRoom(t, srv, inv, "Bob")
	a.waitType("member_joined")
	return
}

func TestCreateAndJoin(t *testing.T) {
	_, srv := newServer(t)
	a, aid, inv := createRoom(t, srv, "Alice", "")
	b := dial(t, srv)
	b.send(msg{"t": "join", "invite": inv, "username": "Bob"})
	j := b.next()
	if j.s("t") != "joined" || j.s("you") == aid {
		t.Fatalf("bad joined: %v", j)
	}
	snap, _ := j["snapshot"].(map[string]any)
	if members, _ := snap["members"].([]any); len(members) != 2 {
		t.Fatalf("snapshot members: %v", snap["members"])
	}
	mj := a.waitType("member_joined")
	mem, _ := mj["member"].(map[string]any)
	if mem["name"] != "Bob" || mem["id"] != j.s("you") {
		t.Fatalf("member_joined: %v", mj)
	}
}

func TestHandshakeErrors(t *testing.T) {
	_, srv := newServer(t)
	_, _, inv := createRoom(t, srv, "Alice", "")

	c := dial(t, srv)
	c.send(msg{"t": "join", "invite": "nope", "username": "X"})
	if m := c.next(); m.s("t") != "error" || m.s("code") != "bad_invite" {
		t.Fatalf("want bad_invite, got %v", m)
	}

	c = dial(t, srv)
	c.send(msg{"t": "join", "invite": inv, "username": "aLiCe"})
	if m := c.next(); m.s("t") != "error" || m.s("code") != "name_taken" {
		t.Fatalf("want name_taken, got %v", m)
	}

	c = dial(t, srv)
	c.send(msg{"t": "ping", "t0": 1})
	if m := c.next(); m.s("t") != "error" || m.s("code") != "not_joined" {
		t.Fatalf("want not_joined, got %v", m)
	}
}

func TestPlayCountdownThenPlaying(t *testing.T) {
	_, a, b, aid, _ := setup(t, "")
	a.send(msg{"t": "cmd", "cid": "c-1", "op": "play", "pos": 10.0})

	sa := a.waitState("countdown", "play")
	sb := b.waitState("countdown", "play")
	for _, s := range []msg{sa, sb} {
		if s.f("startAt") <= s.f("anchorTs") || s.f("anchorPos") != 10 {
			t.Fatalf("bad countdown: %v", s)
		}
		by, _ := s["by"].(map[string]any)
		if by["id"] != aid || by["name"] != "Alice" {
			t.Fatalf("bad by: %v", s["by"])
		}
	}
	if sa.s("ack") != "c-1" {
		t.Fatalf("sender copy ack = %q", sa.s("ack"))
	}
	if _, ok := sb["ack"]; ok {
		t.Fatalf("other copy has ack: %v", sb)
	}
	pa := a.waitState("playing", "start")
	pb := b.waitState("playing", "start")
	if pa.f("anchorTs") != sa.f("startAt") || pb.f("seq") <= sb.f("seq") {
		t.Fatalf("bad playing: %v / %v", pa, pb)
	}
}

func TestPauseActivity(t *testing.T) {
	_, a, b, aid, _ := setup(t, "")
	a.send(msg{"t": "cmd", "cid": "c-1", "op": "play", "pos": 1.0})
	a.waitState("playing", "start")
	b.waitState("playing", "start")

	a.send(msg{"t": "cmd", "cid": "c-2", "op": "pause", "pos": 5.0})
	s := b.waitState("paused", "pause")
	by, _ := s["by"].(map[string]any)
	if by["name"] != "Alice" || by["id"] != aid || s.f("anchorPos") != 5 {
		t.Fatalf("bad paused: %v", s)
	}
	act := b.wait("pause activity", func(m msg) bool { return m.s("t") == "activity" && m.s("kind") == "pause" })
	if !strings.Contains(act.s("text"), "durdurdu") {
		t.Fatalf("activity text: %q", act.s("text"))
	}
	if a.waitState("paused", "pause").s("ack") != "c-2" {
		t.Fatal("missing ack on sender copy")
	}
}

func TestHostOnlyPolicy(t *testing.T) {
	_, a, b, _, _ := setup(t, "")
	a.send(msg{"t": "cmd", "cid": "p1", "op": "set_policy", "policy": "host_only"})
	b.wait("policy", func(m msg) bool { return m.s("t") == "policy" && m.s("policy") == "host_only" })

	b.send(msg{"t": "cmd", "cid": "g1", "op": "play"})
	if d := b.waitDenied("host_only"); d.s("ack") != "g1" {
		t.Fatalf("denied ack: %v", d)
	}
	b.send(msg{"t": "cmd", "cid": "g2", "op": "set_policy", "policy": "free"})
	if d := b.waitDenied("not_host"); d.s("ack") != "g2" {
		t.Fatalf("denied ack: %v", d)
	}
}

func TestPingPong(t *testing.T) {
	_, srv := newServer(t)
	a, _, _ := createRoom(t, srv, "Alice", "")
	before := time.Now().UnixMilli()
	a.send(msg{"t": "ping", "t0": 1730130000123})
	p := a.waitType("pong")
	if int64(p.f("t0")) != 1730130000123 {
		t.Fatalf("t0 not echoed: %v", p)
	}
	if ts := int64(p.f("serverTs")); ts < before || ts > time.Now().UnixMilli()+5 {
		t.Fatalf("serverTs out of range: %v", p)
	}
}

func TestE2EStallHoldAndRelease(t *testing.T) {
	_, a, b, _, bid := setup(t, "")
	a.send(msg{"t": "cmd", "cid": "c-1", "op": "play", "pos": 3.0})
	a.waitState("playing", "start")
	b.waitState("playing", "start")

	b.send(msg{"t": "report", "state": "stalled", "pos": 3.5})
	for _, c := range []*client{a, b} {
		h := c.waitState("hold", "stall")
		wf, _ := h["waitingFor"].([]any)
		if len(wf) != 1 || wf[0] != bid || h.s("reason") != "stall" {
			t.Fatalf("bad hold: %v", h)
		}
	}
	b.send(msg{"t": "report", "state": "ready", "bufferSec": 15.0})
	for _, c := range []*client{a, b} {
		r := c.waitState("countdown", "release")
		if r.f("startAt") <= r.f("anchorTs") {
			t.Fatalf("bad release: %v", r)
		}
		c.waitState("playing", "start")
	}
}

func TestRateLimit(t *testing.T) {
	_, a, _, _, _ := setup(t, "")
	for i, op := range []string{"play", "pause", "play", "pause"} {
		a.send(msg{"t": "cmd", "cid": string(rune('a' + i)), "op": op})
	}
	d := a.waitDenied("rate_limited")
	if d.s("ack") != "d" {
		t.Fatalf("denied ack: %v", d)
	}
}

func TestDisconnectAndRoomCleanup(t *testing.T) {
	h, a, b, _, bid := setup(t, "")
	if h.RoomCount() != 1 {
		t.Fatalf("RoomCount = %d", h.RoomCount())
	}
	b.conn.Close(websocket.StatusNormalClosure, "bye")
	if ml := a.waitType("member_left"); ml.s("id") != bid {
		t.Fatalf("member_left: %v", ml)
	}
	a.conn.Close(websocket.StatusNormalClosure, "bye")
	deadline := time.Now().Add(readTimeout)
	for h.RoomCount() != 0 {
		if time.Now().After(deadline) {
			t.Fatalf("room not removed, RoomCount = %d", h.RoomCount())
		}
		time.Sleep(10 * time.Millisecond)
	}
}
