package hub_test

import (
	"database/sql"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/serkanqltun-hub/koni/server/hub"
	"github.com/serkanqltun-hub/koni/server/store"
	_ "modernc.org/sqlite"
)

// p0.18: member invite codes make friends, pictures come from the server's folder,
// rooms are entered by a friend's call, and the lobby shows who is where.
func TestInvitesAvatarsCallsPresence(t *testing.T) {
	dir := t.TempDir()
	avatars := filepath.Join(dir, "avatars")
	os.MkdirAll(avatars, 0o755)
	for _, n := range []string{"av01.jpg", "av02.jpg", "notes.txt"} {
		os.WriteFile(filepath.Join(avatars, n), []byte("x"), 0o644)
	}
	st, err := store.Open(filepath.Join(dir, "koni.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	srv := httptest.NewServer(hub.New(hub.Config{MinLead: 30 * time.Millisecond, Store: st, Avatars: avatars}).Handler())
	defer srv.Close()

	admin, _ := st.CreateInvite(0, time.Hour)
	if r := handshake(t, srv, msg{"t": "register", "code": admin, "username": "Serkan", "password": "uzun-sifre", "avatar": "yok"}); r.s("code") != "bad_avatar" {
		t.Fatalf("unknown picture: %v", r)
	}
	serkan := handshake(t, srv, msg{"t": "register", "code": admin, "username": "Serkan", "password": "uzun-sifre", "avatar": "av01"})
	ts := serkan.s("token")
	if serkan.s("avatar") != "av01" {
		t.Fatalf("register: %v", serkan)
	}

	// A member's code: "ABCD-EFGH", typed in any case, makes the two friends.
	p := handshake(t, srv, msg{"t": "profile", "auth": ts, "op": "invite"})
	code := p.s("code")
	if len(code) != 9 || code[4] != '-' || len(p["avatars"].([]any)) != 2 {
		t.Fatalf("profile invite: %v", p)
	}
	ayse := handshake(t, srv, msg{"t": "register", "code": strings.ToLower(code), "username": "Ayşe", "password": "uzun-sifre"})
	ta := ayse.s("token")
	if ta == "" {
		t.Fatalf("register with member code: %v", ayse)
	}
	fr := handshake(t, srv, msg{"t": "friends", "auth": ta})
	if f := fr["friends"].([]any); len(f) != 1 || f[0].(map[string]any)["mutual"] != true || bare(f[0].(map[string]any)["name"]) != "Serkan" {
		t.Fatalf("invitee's friends: %v", fr)
	}
	if inv := handshake(t, srv, msg{"t": "profile", "auth": ts})["invites"].([]any); len(inv) != 1 || bare(inv[0].(map[string]any)["usedBy"]) != "Ayşe" {
		t.Fatalf("invite list: %v", inv)
	}
	if r := handshake(t, srv, msg{"t": "profile", "auth": ta, "op": "avatar", "avatar": "notes"}); r.s("code") != "bad_avatar" {
		t.Fatalf("non-picture file: %v", r)
	}
	if r := handshake(t, srv, msg{"t": "profile", "auth": ta, "op": "avatar", "avatar": "av02"}); r.s("avatar") != "av02" {
		t.Fatalf("set avatar: %v", r)
	}

	// Pictures need a session.
	get := func(tok string) int {
		req, _ := http.NewRequest("GET", srv.URL+"/avatars/av02", nil)
		if tok != "" {
			req.Header.Set("Authorization", "Bearer "+tok)
		}
		res, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		res.Body.Close()
		return res.StatusCode
	}
	if get("") != 401 || get(ta) != 200 {
		t.Fatalf("avatar http: %d %d", get(""), get(ta))
	}
	// The relay check (nginx auth_request) wants a session in X-Koni-Token.
	auth := func(tok string) int {
		req, _ := http.NewRequest("GET", srv.URL+"/auth", nil)
		if tok != "" {
			req.Header.Set("X-Koni-Token", tok)
		}
		res, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		res.Body.Close()
		return res.StatusCode
	}
	if auth("") != 401 || auth("yanlis") != 401 || auth(ta) != 204 {
		t.Fatalf("auth http: %d %d %d", auth(""), auth("yanlis"), auth(ta))
	}

	// A stranger can't walk in; a call lets them.
	other, _ := st.CreateInvite(0, time.Hour)
	tc := handshake(t, srv, msg{"t": "register", "code": other, "username": "Cem", "password": "uzun-sifre"}).s("token")
	lobby := dial(t, srv)
	lobby.send(msg{"t": "hello", "auth": ta})
	if h := lobby.next(); h.s("t") != "hello" || h.s("avatar") != "av02" {
		t.Fatalf("hello: %v", h)
	}
	a := dial(t, srv)
	a.send(msg{"t": "create_room", "name": "Cuma", "auth": ts})
	room := a.waitType("room_created")
	a.waitType("joined")
	lobby.wait("Serkan in a room", func(m msg) bool {
		if m.s("t") != "presence" {
			return false
		}
		f := m["friends"].([]any)
		return len(f) == 1 && f[0].(map[string]any)["room"] == "Cuma" && f[0].(map[string]any)["online"] == true
	})
	c := dial(t, srv)
	c.send(msg{"t": "join", "invite": room.s("invite"), "auth": tc})
	if r := c.next(); r.s("code") != "not_invited" {
		t.Fatalf("stranger joined: %v", r)
	}
	a.send(msg{"t": "call", "user": "Cem"})
	if r := a.waitType("error"); r.s("code") != "not_friends" {
		t.Fatalf("call a stranger: %v", r)
	}
	a.send(msg{"t": "call", "user": ayse.s("name") + "#" + ayse.s("tag")})
	a.waitType("called")
	call := lobby.wait("call", func(m msg) bool { return m.s("t") == "presence" && len(m["calls"].([]any)) == 1 })
	cl := call["calls"].([]any)[0].(map[string]any)
	if bare(cl["from"]) != "Serkan" || cl["room"] != "Cuma" || cl["invite"] != room.s("invite") {
		t.Fatalf("call: %v", cl)
	}
	b := dial(t, srv)
	b.send(msg{"t": "join", "invite": room.s("invite"), "auth": ta})
	b.waitType("joined")
	lobby.wait("call consumed", func(m msg) bool { return m.s("t") == "presence" && len(m["calls"].([]any)) == 0 })
}

// A pre-p0.18 database (unique usernames, no tag column) gets tags on open.
func TestStoreMigratesTags(t *testing.T) {
	path := filepath.Join(t.TempDir(), "old.db")
	db, err := sql.Open("sqlite", path)
	if err != nil {
		t.Fatal(err)
	}
	for _, q := range []string{
		`CREATE TABLE users (id INTEGER PRIMARY KEY, username TEXT NOT NULL UNIQUE COLLATE NOCASE, pass_hash TEXT NOT NULL, created_at INTEGER NOT NULL, is_admin INTEGER NOT NULL DEFAULT 0)`,
		`CREATE TABLE friends (user_id INTEGER NOT NULL REFERENCES users(id), friend_id INTEGER NOT NULL REFERENCES users(id), created_at INTEGER NOT NULL, PRIMARY KEY (user_id, friend_id))`,
		`INSERT INTO users(id, username, pass_hash, created_at) VALUES (1, 'Serkan', 'x', 0), (2, 'Ayse', 'x', 0)`,
		`INSERT INTO friends VALUES (1, 2, 0), (2, 1, 0)`,
	} {
		if _, err := db.Exec(q); err != nil {
			t.Fatal(err)
		}
	}
	db.Close()
	st, err := store.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	u, err := st.Lookup("serkan")
	if err != nil || len(u.Tag) != 4 {
		t.Fatalf("lookup after migration: %+v %v", u, err)
	}
	if ok, err := st.Mutual(1, 2); !ok || err != nil {
		t.Fatalf("friends kept: %v %v", ok, err)
	}
}

// p0.22: whoever was in a room may come back while it is open, home lists it under
// "rooms", and the host can close it for everyone.
func TestComeBackAndCloseRoom(t *testing.T) {
	dir := t.TempDir()
	st, err := store.Open(filepath.Join(dir, "koni.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	srv := httptest.NewServer(hub.New(hub.Config{MinLead: 30 * time.Millisecond, Store: st}).Handler())
	defer srv.Close()

	reg := func(code, name string) msg {
		return handshake(t, srv, msg{"t": "register", "code": code, "username": name, "password": "uzun-sifre"})
	}
	root, _ := st.CreateInvite(0, time.Hour)
	serkan := reg(root, "Serkan")
	ayse := reg(handshake(t, srv, msg{"t": "profile", "auth": serkan.s("token"), "op": "invite"}).s("code"), "Ayşe")
	cem := reg(handshake(t, srv, msg{"t": "profile", "auth": ayse.s("token"), "op": "invite"}).s("code"), "Cem") // Ayşe's friend, not Serkan's
	handle := func(m msg) string { return m.s("name") + "#" + m.s("tag") }

	s := dial(t, srv)
	s.send(msg{"t": "create_room", "name": "Cuma", "auth": serkan.s("token")})
	inv := s.waitType("room_created").s("invite")
	s.waitType("joined")
	s.send(msg{"t": "call", "user": handle(ayse)})
	s.waitType("called")
	a := dial(t, srv)
	a.send(msg{"t": "join", "invite": inv, "auth": ayse.s("token")})
	a.waitType("joined")
	a.send(msg{"t": "call", "user": handle(cem)})
	a.waitType("called")
	c := dial(t, srv)
	c.send(msg{"t": "join", "invite": inv, "auth": cem.s("token")})
	c.waitType("joined")

	// Serkan and Ayşe leave; only Cem (not Serkan's friend) is inside.
	s.send(msg{"t": "leave"})
	a.send(msg{"t": "leave"})
	c.waitType("member_left")
	c.waitType("member_left")
	lobby := dial(t, srv)
	lobby.send(msg{"t": "hello", "auth": serkan.s("token")})
	open := lobby.wait("own room listed", func(m msg) bool {
		if m.s("t") != "presence" {
			return false
		}
		r, _ := m["rooms"].([]any)
		return len(r) == 1 && r[0].(map[string]any)["mine"] == true && r[0].(map[string]any)["invite"] == inv
	})
	if r := open["rooms"].([]any)[0].(map[string]any); r["room"] != "Cuma" || len(r["members"].([]any)) != 1 || r["left"] == nil {
		t.Fatalf("room card: %v", r)
	}
	s2 := dial(t, srv)
	s2.send(msg{"t": "join", "invite": inv, "auth": serkan.s("token")})
	s2.waitType("joined")

	// Cem became host when Serkan left; Serkan can't close it, Cem can.
	s2.send(msg{"t": "close_room"})
	if d := s2.waitType("denied"); d.s("reason") != "not_host" {
		t.Fatalf("non-host close: %v", d)
	}
	c.send(msg{"t": "close_room"})
	if e := s2.waitType("error"); e.s("code") != "room_ended" {
		t.Fatalf("closed: %v", e)
	}
	lobby.wait("room gone", func(m msg) bool {
		r, _ := m["rooms"].([]any)
		return m.s("t") == "presence" && len(r) == 0
	})
	s3 := dial(t, srv)
	s3.send(msg{"t": "join", "invite": inv, "auth": serkan.s("token")})
	if r := s3.next(); r.s("code") != "bad_invite" {
		t.Fatalf("join closed room: %v", r)
	}
}

// p0.23: "İzleyeceklerim" / "İzlediklerim": a friend sees your lists and what you
// both want; strangers can't read them; the künye asks who else wants a title.
func TestLists(t *testing.T) {
	st, err := store.Open(filepath.Join(t.TempDir(), "koni.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	srv := httptest.NewServer(hub.New(hub.Config{Store: st}).Handler())
	defer srv.Close()
	root, _ := st.CreateInvite(0, time.Hour)
	serkan := handshake(t, srv, msg{"t": "register", "code": root, "username": "Serkan", "password": "uzun-sifre"})
	ts := serkan.s("token")
	ayse := handshake(t, srv, msg{"t": "register", "code": handshake(t, srv, msg{"t": "profile", "auth": ts, "op": "invite"}).s("code"), "username": "Ayşe", "password": "uzun-sifre"})
	ta := ayse.s("token")
	root2, _ := st.CreateInvite(0, time.Hour)
	tc := handshake(t, srv, msg{"t": "register", "code": root2, "username": "Cem", "password": "uzun-sifre"}).s("token")

	add := func(tok, list, ref, title string) msg {
		return handshake(t, srv, msg{"t": "lists", "auth": tok, "op": "add", "list": list, "entry": msg{"ref": ref, "title": title, "kind": "movie", "poster": "https://img/x.jpg"}})
	}
	add(ts, "want", "tt2543164", "Arrival")
	add(ts, "want", "tt6751668", "Parasite")
	add(ta, "want", "tt2543164", "Arrival")
	mine := add(ts, "seen", "tt6751668", "Parasite") // seen takes it off "want"
	if w := mine["want"].([]any); len(w) != 1 || w[0].(map[string]any)["ref"] != "tt2543164" || len(w[0].(map[string]any)["mates"].([]any)) != 1 {
		t.Fatalf("own lists: %v", mine)
	}
	if s := mine["seen"].([]any); len(s) != 1 {
		t.Fatalf("seen: %v", mine)
	}
	if r := handshake(t, srv, msg{"t": "lists", "auth": ts, "op": "add", "list": "want", "entry": msg{"ref": "", "title": "x"}}); r.s("code") != "bad_entry" {
		t.Fatalf("empty ref: %v", r)
	}
	handshake(t, srv, msg{"t": "lists", "auth": ts, "op": "rate", "ref": "tt6751668", "rating": 1})

	f := handshake(t, srv, msg{"t": "lists", "auth": ta, "user": serkan.s("name") + "#" + serkan.s("tag")})
	if c := f["common"].([]any); len(c) != 1 || c[0] != "tt2543164" || f["seen"].([]any)[0].(map[string]any)["rating"] != 1.0 {
		t.Fatalf("friend view: %v", f)
	}
	if r := handshake(t, srv, msg{"t": "lists", "auth": tc, "user": serkan.s("name") + "#" + serkan.s("tag")}); r.s("code") != "not_friends" {
		t.Fatalf("stranger read lists: %v", r)
	}
	w := handshake(t, srv, msg{"t": "lists", "auth": ta, "op": "who", "ref": "tt6751668"})
	if s := w["seen"].([]any); len(s) != 1 || bare(s[0].(map[string]any)["name"]) != "Serkan" {
		t.Fatalf("who: %v", w)
	}
}
