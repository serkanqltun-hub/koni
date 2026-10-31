package hub_test

import (
	"net/http/httptest"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/serkanqltun-hub/koni/server/hub"
	"github.com/serkanqltun-hub/koni/server/store"
)

// Watch time counts only while the room plays, night time is 00–04 Turkey time, and
// stats show only the caller and their mutual friends.
func TestStatsWatchTimeAndBadges(t *testing.T) {
	var mu sync.Mutex
	now := time.Date(2026, 10, 30, 1, 0, 0, 0, time.FixedZone("TRT", 3*3600))
	clock := func() time.Time { mu.Lock(); defer mu.Unlock(); return now }
	advance := func(d time.Duration) { mu.Lock(); now = now.Add(d); mu.Unlock() }

	st, err := store.Open(filepath.Join(t.TempDir(), "koni.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	srv := httptest.NewServer(hub.New(hub.Config{MinLead: 30 * time.Millisecond, ReadyLead: 30 * time.Millisecond, Store: st, Now: clock}).Handler())
	defer srv.Close()
	token := func(name string) string {
		code, _ := st.CreateInvite(0, time.Hour)
		r := handshake(t, srv, msg{"t": "register", "code": code, "username": name, "password": "uzun-sifre"})
		return r.s("token")
	}
	ta, tb, tc := token("Ayse"), token("Berk"), token("Cem")
	// p0.18: a room is entered as a mutual friend of someone inside; the friendship is
	// dropped again before the sharing checks below.
	for _, f := range [][2]string{{ta, "Berk"}, {tb, "Ayse"}} {
		handshake(t, srv, msg{"t": "friends", "auth": f[0], "op": "add", "user": f[1]})
	}

	a := dial(t, srv)
	a.send(msg{"t": "create_room", "name": "Oda", "auth": ta})
	created := a.waitType("room_created")
	a.waitType("joined")
	b := dial(t, srv)
	b.send(msg{"t": "join", "invite": created.s("invite"), "auth": tb})
	b.waitType("joined")
	for _, f := range [][2]string{{ta, "Berk"}, {tb, "Ayse"}} {
		handshake(t, srv, msg{"t": "friends", "auth": f[0], "op": "remove", "user": f[1]})
	}

	a.send(msg{"t": "cmd", "cid": "p1", "op": "play", "pos": 0.0})
	a.waitState("playing", "start")
	advance(30 * time.Minute) // 01:00–01:30, all night
	a.send(msg{"t": "cmd", "cid": "p2", "op": "pause", "pos": 1800.0})
	a.waitState("paused", "pause")
	advance(time.Hour) // paused: not watching
	b.send(msg{"t": "cmd", "cid": "s1", "op": "seek", "pos": 60.0})
	b.waitState("countdown", "seek") // seek restarts playback; the clock stays put
	a.send(msg{"t": "leave"})
	b.send(msg{"t": "leave"})

	if r := handshake(t, srv, msg{"t": "stats", "auth": ta, "period": "decade"}); r.s("code") != "bad_period" {
		t.Fatalf("bad period: %v", r)
	}
	friends := func(tok string, m msg) msg {
		m["t"], m["auth"] = "friends", tok
		r := handshake(t, srv, m)
		if r.s("t") != "friends" {
			t.Fatalf("friends %v: %v", m, r)
		}
		return r
	}
	statsUsers := func(tok string) int {
		users, _ := handshake(t, srv, msg{"t": "stats", "auth": tok, "period": "all"})["users"].([]any)
		return len(users)
	}
	// Sharing a room is not enough, and neither is a one-way add.
	if n := statsUsers(ta); n != 1 {
		t.Fatalf("no friends yet, Ayse sees %d users", n)
	}
	if r := handshake(t, srv, msg{"t": "friends", "auth": ta, "op": "add", "user": "Yok"}); r.s("code") != "unknown_user" {
		t.Fatalf("unknown user: %v", r)
	}
	if r := handshake(t, srv, msg{"t": "friends", "auth": ta, "op": "add", "user": "ayse"}); r.s("code") != "self_friend" {
		t.Fatalf("self: %v", r)
	}
	friends(ta, msg{"op": "add", "user": "berk"})
	if n := statsUsers(ta); n != 1 {
		t.Fatalf("one-way add, Ayse sees %d users", n)
	}
	if r := friends(tb, msg{"op": "list"}); len(r["requests"].([]any)) != 1 || bare(r["requests"].([]any)[0].(map[string]any)["name"]) != "Ayse" {
		t.Fatalf("Berk's requests: %v", r)
	}
	r := friends(tb, msg{"op": "add", "user": "Ayse"})
	if f := r["friends"].([]any); len(f) != 1 || f[0].(map[string]any)["mutual"] != true || len(r["requests"].([]any)) != 0 {
		t.Fatalf("Berk's list after adding back: %v", r)
	}
	friends(tc, msg{"op": "add", "user": "Ayse"}) // Cem → Ayse only: no sharing

	var got msg
	deadline := time.Now().Add(5 * time.Second) // events are written about once a second
	for {
		got = handshake(t, srv, msg{"t": "stats", "auth": ta, "period": "all"})
		if got.s("t") != "stats" {
			t.Fatalf("stats: %v", got)
		}
		if users, _ := got["users"].([]any); len(users) == 2 && users[0].(map[string]any)["watchSec"].(float64) > 0 && users[1].(map[string]any)["watchSec"].(float64) > 0 {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("watch time never recorded: %v", got)
		}
		time.Sleep(100 * time.Millisecond)
	}
	byName := map[string]map[string]any{}
	for _, u := range got["users"].([]any) {
		m := u.(map[string]any)
		byName[bare(m["user"])] = m
	}
	if _, ok := byName["Cem"]; ok {
		t.Fatalf("Cem is not a mutual friend but is visible: %v", got["users"])
	}
	for _, name := range []string{"Ayse", "Berk"} {
		u := byName[name]
		if u["watchSec"] != 1800.0 || u["nightSec"] != 1800.0 || u["longestSec"] != 1800.0 {
			t.Fatalf("%s: %v", name, u)
		}
	}
	if byName["Ayse"]["pauses"] != 1.0 || byName["Berk"]["seeks"] != 1.0 {
		t.Fatalf("counts: %v", got["users"])
	}
	badges := map[string][]any{}
	for _, b := range got["badges"].([]any) {
		m := b.(map[string]any)
		users, _ := m["users"].([]any)
		badges[m["id"].(string)] = users
	}
	if u := badges["pause"]; len(u) != 1 || bare(u[0]) != "Ayse" {
		t.Fatalf("pause badge: %v", u)
	}
	if u := badges["night"]; len(u) != 2 {
		t.Fatalf("night badge shared by the tie: %v", u)
	}
	if u := badges["marathon"]; len(u) != 0 {
		t.Fatalf("30 min is no marathon: %v", u)
	}

	// Cem added Ayse but she didn't add him back: he sees only himself.
	if r := handshake(t, srv, msg{"t": "stats", "auth": tc, "period": "week"}); len(r["users"].([]any)) != 1 {
		t.Fatalf("Cem's view: %v", r)
	}
	// Removing ends the sharing both ways.
	friends(tb, msg{"op": "remove", "user": "Ayse"})
	if n := statsUsers(ta); n != 1 {
		t.Fatalf("after remove Ayse sees %d users", n)
	}
	if r := friends(ta, msg{}); r["friends"].([]any)[0].(map[string]any)["mutual"] != false {
		t.Fatalf("Ayse's list after Berk removed her: %v", r)
	}
	// Ayse declines Cem's request: it leaves her list.
	if r := friends(ta, msg{"op": "decline", "user": "Cem"}); len(r["requests"].([]any)) != 0 {
		t.Fatalf("after decline: %v", r)
	}
}

// bare drops the "#1234" tag of an account handle.
func bare(v any) string {
	s, _ := v.(string)
	if i := strings.LastIndexByte(s, '#'); i > 0 {
		return s[:i]
	}
	return s
}

// Time on a Lounge (queue) song is listening, not watching.
func TestStatsListenTimeFromQueue(t *testing.T) {
	var mu sync.Mutex
	now := time.Date(2026, 10, 30, 20, 0, 0, 0, time.FixedZone("TRT", 3*3600))
	clock := func() time.Time { mu.Lock(); defer mu.Unlock(); return now }
	advance := func(d time.Duration) { mu.Lock(); now = now.Add(d); mu.Unlock() }

	st, err := store.Open(filepath.Join(t.TempDir(), "koni.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	srv := httptest.NewServer(hub.New(hub.Config{MinLead: 30 * time.Millisecond, ReadyLead: 30 * time.Millisecond, Store: st, Now: clock}).Handler())
	defer srv.Close()
	code, _ := st.CreateInvite(0, time.Hour)
	ta := handshake(t, srv, msg{"t": "register", "code": code, "username": "Ayse", "password": "uzun-sifre"}).s("token")

	a := dial(t, srv)
	a.send(msg{"t": "create_room", "name": "Lounge", "auth": ta})
	a.waitType("joined")
	a.send(msg{"t": "queue_add", "media": msg{"kind": "url", "src": "https://youtu.be/one", "title": "Bir"}})
	a.wait("queued song is room media", func(m msg) bool { return m.s("t") == "state" && mediaSrc(m) == "https://youtu.be/one" })
	a.send(msg{"t": "cmd", "cid": "p1", "op": "play", "pos": 0.0})
	a.waitState("playing", "start")
	advance(10 * time.Minute)
	a.send(msg{"t": "leave"})

	deadline := time.Now().Add(5 * time.Second)
	for {
		got := handshake(t, srv, msg{"t": "stats", "auth": ta, "period": "all"})
		if users, _ := got["users"].([]any); len(users) == 1 {
			u := users[0].(map[string]any)
			if u["listenSec"] == 600.0 {
				if u["watchSec"] != 0.0 {
					t.Fatalf("queue time counted as watching: %v", u)
				}
				return
			}
		}
		if time.Now().After(deadline) {
			t.Fatalf("listen time never recorded: %v", got)
		}
		time.Sleep(100 * time.Millisecond)
	}
}

func TestNightMsAcrossMidnight(t *testing.T) {
	trt := time.FixedZone("TRT", 3*3600)
	from := time.Date(2026, 10, 29, 23, 0, 0, 0, trt)
	// 23:00 → next day 05:00 contains 00:00–04:00 once.
	if got := store.NightMs(from.UnixMilli(), from.Add(6*time.Hour).UnixMilli()); got != (4 * time.Hour).Milliseconds() {
		t.Fatalf("night = %v", time.Duration(got)*time.Millisecond)
	}
	// 03:30 → 04:30 contains 30 min.
	from = time.Date(2026, 10, 30, 3, 30, 0, 0, trt)
	if got := store.NightMs(from.UnixMilli(), from.Add(time.Hour).UnixMilli()); got != (30 * time.Minute).Milliseconds() {
		t.Fatalf("night = %v", time.Duration(got)*time.Millisecond)
	}
}
