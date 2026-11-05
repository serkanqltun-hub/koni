package hub_test

import (
	"net/http/httptest"
	"path/filepath"
	"sync"
	"testing"
	"time"

	"github.com/serkanqltun-hub/koni/server/hub"
	"github.com/serkanqltun-hub/koni/server/store"
)

// Film night: planned for mutual friends, shown on presence, opened at its time;
// guests get in, others don't.
func TestFilmNight(t *testing.T) {
	var mu sync.Mutex
	now := time.Date(2026, 11, 6, 20, 0, 0, 0, time.FixedZone("TRT", 3*3600))
	clock := func() time.Time { mu.Lock(); defer mu.Unlock(); return now }
	advance := func(d time.Duration) { mu.Lock(); now = now.Add(d); mu.Unlock() }
	st, err := store.Open(filepath.Join(t.TempDir(), "koni.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	srv := httptest.NewServer(hub.New(hub.Config{MinLead: 30 * time.Millisecond, Store: st, Now: clock}).Handler())
	defer srv.Close()

	root, _ := st.CreateInvite(0, time.Hour)
	ayse := handshake(t, srv, msg{"t": "register", "code": root, "username": "Ayse", "password": "uzun-sifre"})
	ta := ayse.s("token")
	code := handshake(t, srv, msg{"t": "profile", "auth": ta, "op": "invite"}).s("code")
	berk := handshake(t, srv, msg{"t": "register", "code": code, "username": "Berk", "password": "uzun-sifre"})
	tb := berk.s("token")
	root2, _ := st.CreateInvite(0, time.Hour)
	cem := handshake(t, srv, msg{"t": "register", "code": root2, "username": "Cem", "password": "uzun-sifre"})
	tc := cem.s("token")

	at := now.Add(2 * time.Hour).UnixMilli()
	if r := handshake(t, srv, msg{"t": "event", "auth": ta, "op": "create", "at": at, "guests": []string{"Cem#" + cem.s("tag")}}); r.s("code") != "not_friends" {
		t.Fatalf("stranger guest: %v", r)
	}
	if r := handshake(t, srv, msg{"t": "event", "auth": ta, "op": "create", "at": now.Add(-time.Minute).UnixMilli()}); r.s("code") != "bad_entry" {
		t.Fatalf("past: %v", r)
	}
	r := ok(t, handshake(t, srv, msg{"t": "event", "auth": ta, "op": "create", "title": "Cuma filmi", "at": at, "guests": []string{"Berk#" + berk.s("tag")}}))
	id := r.f("id")
	if evs := r["events"].([]any); id == 0 || len(evs) != 1 || evs[0].(map[string]any)["mine"] != true {
		t.Fatalf("create: %v", r)
	}
	r = ok(t, handshake(t, srv, msg{"t": "event", "auth": tb, "op": "rsvp", "id": id, "rsvp": "yes"}))
	ev := r["events"].([]any)[0].(map[string]any)
	if ev["mine"] == true || ev["title"] != "Cuma filmi" || ev["guests"].([]any)[0].(map[string]any)["rsvp"] != "yes" || ev["room"] != nil {
		t.Fatalf("guest's view: %v", r)
	}
	if r := handshake(t, srv, msg{"t": "event", "auth": tc}); len(r["events"].([]any)) != 0 {
		t.Fatalf("Cem sees: %v", r)
	}

	lobby := dial(t, srv)
	lobby.send(msg{"t": "hello", "auth": tb})
	lobby.waitType("hello")
	lobby.wait("presence with the night", func(m msg) bool {
		evs, _ := m["events"].([]any)
		return m.s("t") == "presence" && len(evs) == 1
	})

	advance(2*time.Hour + time.Minute)
	handshake(t, srv, msg{"t": "event", "auth": tb, "op": "rsvp", "id": id, "rsvp": "yes"}) // pokes presence
	p := lobby.wait("the night's room", func(m msg) bool {
		evs, _ := m["events"].([]any)
		return m.s("t") == "presence" && len(evs) == 1 && evs[0].(map[string]any)["room"] != nil
	})
	room := p["events"].([]any)[0].(map[string]any)["room"].(map[string]any)
	if calls := p["calls"].([]any); len(calls) != 1 || calls[0].(map[string]any)["invite"] != room["invite"] || room["name"] != "Cuma filmi" {
		t.Fatalf("call: %v", p)
	}

	b := dial(t, srv)
	b.send(msg{"t": "join", "invite": room["invite"], "auth": tb})
	b.waitType("joined")
	c := dial(t, srv)
	c.send(msg{"t": "join", "invite": room["invite"], "auth": tc})
	if e := c.waitType("error"); e.s("code") != "not_invited" {
		t.Fatalf("Cem: %v", e)
	}
	a := dial(t, srv)
	a.send(msg{"t": "join", "invite": room["invite"], "auth": ta})
	a.waitType("joined")
}

func TestRecapAndVideoList(t *testing.T) {
	st, err := store.Open(filepath.Join(t.TempDir(), "koni.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	srv := httptest.NewServer(hub.New(hub.Config{Store: st}).Handler())
	defer srv.Close()
	root, _ := st.CreateInvite(0, time.Hour)
	tok := handshake(t, srv, msg{"t": "register", "code": root, "username": "Ayse", "password": "uzun-sifre"}).s("token")
	if r := handshake(t, srv, msg{"t": "recap", "auth": tok, "month": "2019-13"}); r.s("code") != "bad_period" {
		t.Fatalf("bad month: %v", r)
	}
	r := ok(t, handshake(t, srv, msg{"t": "recap", "auth": tok}))
	if r.s("month") == "" || r.f("peakHour") != -1 || len(r["hours"].([]any)) != 24 || r["best"] != nil {
		t.Fatalf("empty recap: %v", r)
	}
	ok(t, handshake(t, srv, msg{"t": "lists", "auth": tok, "op": "add", "list": "want",
		"entry": msg{"ref": "yt:dQw4w9WgXcQ", "kind": "video", "title": "Bir video", "poster": "https://i.ytimg.com/vi/x/hq.jpg"}}))
	l := ok(t, handshake(t, srv, msg{"t": "lists", "auth": tok}))
	want := l["want"].([]any)
	if len(want) != 1 || want[0].(map[string]any)["kind"] != "video" {
		t.Fatalf("video on the list: %v", l)
	}
}
