package hub_test

import (
	"net/http/httptest"
	"path/filepath"
	"testing"
	"time"

	"github.com/serkanqltun-hub/koni/server/discover"
	"github.com/serkanqltun-hub/koni/server/hub"
	"github.com/serkanqltun-hub/koni/server/store"
)

// soloServer: an account server with the fake TMDB and two mutual friends.
func soloServer(t *testing.T) (srv *httptest.Server, ta, tb string) {
	t.Helper()
	d, err := discover.New("test-key", "")
	if err != nil {
		t.Fatal(err)
	}
	d.TMDBBase = fakeTMDB(t).URL
	st, err := store.Open(filepath.Join(t.TempDir(), "koni.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { st.Close() })
	srv = httptest.NewServer(hub.New(hub.Config{MinLead: 30 * time.Millisecond, Store: st, Discover: d}).Handler())
	t.Cleanup(srv.Close)
	code, _ := st.CreateInvite(0, time.Hour)
	ta = handshake(t, srv, msg{"t": "register", "code": code, "username": "Ayse", "password": "uzun-sifre"}).s("token")
	member := handshake(t, srv, msg{"t": "profile", "auth": ta, "op": "invite"}).s("code")
	tb = handshake(t, srv, msg{"t": "register", "code": member, "username": "Berk", "password": "uzun-sifre"}).s("token")
	if ta == "" || tb == "" {
		t.Fatal("register failed")
	}
	return srv, ta, tb
}

func addList(t *testing.T, srv *httptest.Server, tok, list, ref string) {
	t.Helper()
	r := handshake(t, srv, msg{"t": "lists", "auth": tok, "op": "add", "list": list, "entry": msg{"ref": ref, "kind": "movie", "title": ref}})
	if r.s("t") == "error" {
		t.Fatalf("lists add: %v", r)
	}
}

// p0.26 `solo_night`: ten detailed cards without seen titles, the rest via
// `solo_night_pick`; a short time budget drops longer films.
func TestSoloNight(t *testing.T) {
	srv, ta, _ := soloServer(t)
	if r := handshake(t, srv, msg{"t": "solo_night", "kind": "movie"}); r.s("code") != "auth_required" {
		t.Fatalf("no auth: %v", r)
	}
	addList(t, srv, ta, "seen", "tt100")
	addList(t, srv, ta, "seen", "tmdb:movie:101")

	r := handshake(t, srv, msg{"t": "solo_night", "auth": ta, "kind": "movie", "filter": msg{"genres": []int{53}}, "budgetMin": 150, "dub": "tr"})
	cards := items(t, r)
	if len(cards) != 10 || r.f("more") != 0 || r.s("dub") != "tr" || r.s("error") != "" {
		t.Fatalf("solo_night: %v", r)
	}
	for _, c := range cards {
		if c["id"] == 100.0 || c["id"] == 101.0 || c["runtime"] != 120.0 || c["imdb"] == "" {
			t.Fatalf("card: %v", c)
		}
	}
	// 12 films − 2 seen = 10 dealt, nothing left
	if p := handshake(t, srv, msg{"t": "solo_night_pick", "auth": ta, "ref": "tt102"}); p["item"] != nil || p.f("more") != 0 {
		t.Fatalf("pick at the end: %v", p)
	}

	// without the seen list there are 12: two wait for pick
	addList(t, srv, ta, "want", "tt105") // want doesn't filter
	handshake(t, srv, msg{"t": "lists", "auth": ta, "op": "remove", "list": "seen", "ref": "tt100"})
	r = handshake(t, srv, msg{"t": "solo_night", "auth": ta, "kind": "movie"})
	if len(items(t, r)) != 10 || r.f("more") != 1 {
		t.Fatalf("solo_night more: %v", r)
	}
	p := handshake(t, srv, msg{"t": "solo_night_pick", "auth": ta, "ref": "tt103"})
	if it, _ := p["item"].(map[string]any); it == nil || p.f("more") != 0 || p.s("ref") != "tt103" {
		t.Fatalf("pick: %v", p)
	}

	if r := handshake(t, srv, msg{"t": "solo_night", "auth": ta, "kind": "movie", "budgetMin": 90}); len(items(t, r)) != 0 {
		t.Fatalf("budget: %v", r)
	}
	if r := handshake(t, srv, msg{"t": "solo_night", "auth": ta, "kind": "music"}); r.s("error") == "" {
		t.Fatalf("bad kind: %v", r)
	}
}

// p0.26 `browse`: 30 titles a page, without discover an error line.
func TestBrowse(t *testing.T) {
	srv, ta, _ := soloServer(t)
	r := handshake(t, srv, msg{"t": "browse", "auth": ta, "kind": "movie", "genre": 53, "sort": "rating", "page": 1})
	if r.s("t") != "browse" || len(items(t, r)) != 12 || r.f("page") != 1 || r.s("error") != "" {
		t.Fatalf("browse: %v", r)
	}
	off := httptest.NewServer(hub.New(hub.Config{}).Handler())
	defer off.Close()
	if r := handshake(t, off, msg{"t": "browse", "kind": "movie"}); r.s("error") == "" || len(items(t, r)) != 0 {
		t.Fatalf("browse off: %v", r)
	}
}

// p0.26 night_start pool:"lists": the deck is the members' shared want titles.
func TestNightPoolLists(t *testing.T) {
	srv, ta, tb := soloServer(t)
	addList(t, srv, ta, "want", "tmdb:movie:105")
	addList(t, srv, ta, "want", "tmdb:movie:106")
	addList(t, srv, tb, "want", "tmdb:movie:105")

	a := dial(t, srv)
	a.send(msg{"t": "create_room", "name": "Oda", "auth": ta})
	created := a.waitType("room_created")
	b := dial(t, srv)
	b.send(msg{"t": "join", "invite": created.s("invite"), "auth": tb})
	b.waitType("joined")

	a.send(msg{"t": "night_start", "kind": "movie", "perUser": 2, "pool": "lists"})
	if n := waitNight(b, "setup"); n.s("source") != "lists" {
		t.Fatalf("start: %v", n)
	}
	deck := b.waitType("deck")
	if its := items(t, deck); len(its) != 1 || its[0]["id"] != 105.0 {
		t.Fatalf("deck: %v", deck)
	}
	waitNight(a, "vote")

	// nobody wants a series: the night stays in setup
	a.send(msg{"t": "night_start", "kind": "tv", "perUser": 2, "pool": "lists"})
	a.waitType("fun_error")
	if n := waitNight(a, "setup"); n.s("source") != "" {
		t.Fatalf("empty lists: %v", n)
	}
}
