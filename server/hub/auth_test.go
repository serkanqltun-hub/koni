package hub_test

import (
	"context"
	"database/sql"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/serkanqltun-hub/koni/server/hub"
	"github.com/serkanqltun-hub/koni/server/store"
)

// handshake sends one first message and returns the single reply.
func handshake(t *testing.T, srv *httptest.Server, m msg) msg {
	t.Helper()
	c := dial(t, srv)
	c.send(m)
	return c.next()
}

func TestAccounts(t *testing.T) {
	path := filepath.Join(t.TempDir(), "koni.db")
	st, err := store.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	srv := httptest.NewServer(hub.New(hub.Config{MinLead: 30 * time.Millisecond, Store: st}).Handler())
	defer srv.Close()
	code, _ := st.CreateInvite(0, time.Hour)
	expired, _ := st.CreateInvite(0, -time.Second)

	errCode := func(m msg) string {
		if m.s("t") != "error" {
			t.Fatalf("expected error, got %v", m)
		}
		return m.s("code")
	}
	if c := errCode(handshake(t, srv, msg{"t": "create_room", "username": "Serkan"})); c != "auth_required" {
		t.Fatalf("no token: %s", c)
	}
	if c := errCode(handshake(t, srv, msg{"t": "register", "code": expired, "username": "Serkan", "password": "uzun-sifre"})); c != "bad_code" {
		t.Fatalf("expired invite: %s", c)
	}
	if c := errCode(handshake(t, srv, msg{"t": "register", "code": code, "username": "Serkan", "password": "kisa"})); c != "bad_password" {
		t.Fatalf("short password: %s", c)
	}
	reg := handshake(t, srv, msg{"t": "register", "code": code, "username": " Serkan ", "password": "uzun-sifre"})
	if reg.s("t") != "auth" || reg.s("token") == "" || reg.s("name") != "Serkan" {
		t.Fatalf("register: %v", reg)
	}
	other, _ := st.CreateInvite(0, time.Hour)
	if c := errCode(handshake(t, srv, msg{"t": "register", "code": code, "username": "Coni", "password": "uzun-sifre"})); c != "bad_code" {
		t.Fatalf("reused invite: %s", c)
	}
	if c := errCode(handshake(t, srv, msg{"t": "login", "username": "serkan", "password": "yanlis-sifre"})); c != "bad_login" {
		t.Fatalf("wrong password: %s", c)
	}
	// Names may repeat (p0.18): the #tag tells them apart; a bare name logs in only
	// while it is unique.
	twin := handshake(t, srv, msg{"t": "register", "code": other, "username": "serkan", "password": "baska-sifre"})
	if twin.s("t") != "auth" || twin.s("tag") == reg.s("tag") || len(twin.s("tag")) != 4 {
		t.Fatalf("same name, new tag: %v / %v", twin, reg)
	}
	if c := errCode(handshake(t, srv, msg{"t": "login", "username": "serkan", "password": "uzun-sifre"})); c != "bad_login" {
		t.Fatalf("ambiguous bare name: %s", c)
	}
	login := handshake(t, srv, msg{"t": "login", "username": "Serkan#" + reg.s("tag"), "password": "uzun-sifre"})
	if login.s("t") != "auth" || login.s("token") == "" || login.s("token") == reg.s("token") || login.s("name") != "Serkan" {
		t.Fatalf("login: %v", login)
	}
	// The account name wins over the typed username; events carry the account id.
	a := dial(t, srv)
	a.send(msg{"t": "create_room", "name": "Oda", "username": "baskasi", "auth": login.s("token")})
	a.waitType("room_created")
	j := a.waitType("joined")
	members := j["snapshot"].(map[string]any)["members"].([]any)
	if name := members[0].(map[string]any)["name"]; name != "Serkan" {
		t.Fatalf("member name %v", name)
	}
	a.send(msg{"t": "cmd", "cid": "c1", "op": "pause", "pos": 12.5})
	a.waitState("paused", "pause")
	a.send(msg{"t": "leave"})
	for { // the server closes the connection after leave
		ctx, cancel := context.WithTimeout(context.Background(), readTimeout)
		_, _, err := a.conn.Read(ctx)
		cancel()
		if err != nil {
			break
		}
	}

	// logout invalidates that session; the register session still works
	if r := handshake(t, srv, msg{"t": "logout", "auth": login.s("token")}); r.s("t") != "auth" {
		t.Fatalf("logout: %v", r)
	}
	if c := errCode(handshake(t, srv, msg{"t": "create_room", "auth": login.s("token")})); c != "bad_token" {
		t.Fatalf("after logout: %s", c)
	}
	if m := handshake(t, srv, msg{"t": "create_room", "auth": reg.s("token")}); m.s("t") != "room_created" {
		t.Fatalf("other session: %v", m)
	}

	// Close flushes the batched events.
	if err := st.Close(); err != nil {
		t.Fatal(err)
	}
	db, err := sql.Open("sqlite", path)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	rows, err := db.Query(`SELECT type, coalesce(user_id, 0), payload FROM events ORDER BY id`)
	if err != nil {
		t.Fatal(err)
	}
	var got []string
	for rows.Next() {
		var typ, payload string
		var user int64
		rows.Scan(&typ, &user, &payload)
		if user != 1 {
			t.Errorf("%s: user %d", typ, user)
		}
		got = append(got, typ)
	}
	want := "room_create join pause leave room_create join"
	if s := strings.Join(got, " "); s != want {
		t.Fatalf("events %q, want %q", s, want)
	}
}
