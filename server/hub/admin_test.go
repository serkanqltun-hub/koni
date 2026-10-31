package hub_test

import (
	"encoding/base64"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/serkanqltun-hub/koni/server/hub"
	"github.com/serkanqltun-hub/koni/server/store"
)

// p0.19: admins list rooms and members, close rooms, ban members (inviter loses the
// invite right) and upload their own picture.
func TestAdmin(t *testing.T) {
	dir := t.TempDir()
	avatars := filepath.Join(dir, "avatars")
	os.MkdirAll(avatars, 0o755)
	os.WriteFile(filepath.Join(avatars, "av01.jpg"), []byte("x"), 0o644)
	st, err := store.Open(filepath.Join(dir, "koni.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	srv := httptest.NewServer(hub.New(hub.Config{MinLead: 30 * time.Millisecond, Store: st, Avatars: avatars}).Handler())
	defer srv.Close()

	reg := func(code, name string) string {
		r := handshake(t, srv, msg{"t": "register", "code": code, "username": name, "password": "uzun-sifre"})
		if r.s("token") == "" {
			t.Fatalf("register %s: %v", name, r)
		}
		return r.s("token")
	}
	c0, _ := st.CreateInvite(0, time.Hour)
	ts := reg(c0, "Serkan")
	if _, err := st.SetAdmin("Serkan", true); err != nil {
		t.Fatal(err)
	}
	// Burak (not admin) invites Kaan.
	c1, _ := st.CreateInvite(0, time.Hour)
	tb := reg(c1, "Burak")
	kaanCode := handshake(t, srv, msg{"t": "profile", "auth": tb, "op": "invite"}).s("code")
	tk := reg(kaanCode, "Kaan")
	handshake(t, srv, msg{"t": "profile", "auth": tb, "op": "invite"}) // an open code that ban cancels

	if r := handshake(t, srv, msg{"t": "admin", "auth": tb}); r.s("code") != "forbidden" {
		t.Fatalf("non-admin: %v", r)
	}
	if p := handshake(t, srv, msg{"t": "profile", "auth": ts}); p["admin"] != true {
		t.Fatalf("profile admin flag: %v", p)
	}

	// Kaan opens a room; the admin sees it with Kaan inside.
	k := dial(t, srv)
	k.send(msg{"t": "create_room", "name": "test", "auth": tk})
	room := k.waitType("room_created")
	k.waitType("joined")
	a := handshake(t, srv, msg{"t": "admin", "auth": ts})
	rooms := a["rooms"].([]any)
	if len(rooms) != 1 || rooms[0].(map[string]any)["name"] != "test" || len(a["members"].([]any)) != 3 {
		t.Fatalf("admin list: %v", a)
	}

	// Ban Kaan: kicked from the room, account gone, Burak loses invites.
	handshake(t, srv, msg{"t": "admin", "auth": ts, "op": "ban", "user": "Kaan"})
	if e := k.waitType("error"); e.s("code") != "banned" {
		t.Fatalf("kick: %v", e)
	}
	if r := handshake(t, srv, msg{"t": "friends", "auth": tk}); r.s("code") != "bad_token" {
		t.Fatalf("banned session: %v", r)
	}
	if r := handshake(t, srv, msg{"t": "profile", "auth": tb, "op": "invite"}); r.s("code") != "no_invites" {
		t.Fatalf("inviter keeps invites: %v", r)
	}
	if p := handshake(t, srv, msg{"t": "profile", "auth": tb}); p["canInvite"] != false || len(p["invites"].([]any)) != 0 {
		t.Fatalf("inviter profile: %v", p)
	}
	handshake(t, srv, msg{"t": "admin", "auth": ts, "op": "invite_right", "user": "Burak", "on": true})
	if r := handshake(t, srv, msg{"t": "profile", "auth": tb, "op": "invite"}); r.s("code") == "" {
		t.Fatalf("right given back: %v", r)
	}

	// Close a room: everyone inside goes home.
	b := dial(t, srv)
	b.send(msg{"t": "create_room", "name": "Cuma", "auth": tb})
	cuma := b.waitType("room_created")
	b.waitType("joined")
	handshake(t, srv, msg{"t": "admin", "auth": ts, "op": "close", "room": cuma.s("room")})
	if e := b.waitType("error"); e.s("code") != "room_closed" {
		t.Fatalf("close: %v", e)
	}
	_ = room

	// Admin picture: added to the picker; while the admin wears it nobody else can.
	png := base64.StdEncoding.EncodeToString([]byte("\x89PNG\r\n\x1a\n0000rest-of-file"))
	if r := handshake(t, srv, msg{"t": "profile", "auth": tb, "op": "upload", "image": png}); r.s("code") != "forbidden" {
		t.Fatalf("member upload: %v", r)
	}
	p := handshake(t, srv, msg{"t": "profile", "auth": ts, "op": "upload", "image": png})
	id := p.s("avatar")
	if len(id) < 4 || id[:3] != "ad-" || len(p["avatars"].([]any)) != 2 {
		t.Fatalf("upload: %v", p)
	}
	req, _ := http.NewRequest("GET", srv.URL+"/avatars/"+id, nil)
	req.Header.Set("Authorization", "Bearer "+tb)
	res, err := http.DefaultClient.Do(req)
	if err != nil || res.StatusCode != 200 {
		t.Fatalf("serve admin picture: %v %v", res, err)
	}
	res.Body.Close()
	if p := handshake(t, srv, msg{"t": "profile", "auth": tb}); len(p["taken"].([]any)) != 1 {
		t.Fatalf("taken list: %v", p)
	}
	if r := handshake(t, srv, msg{"t": "profile", "auth": tb, "op": "avatar", "avatar": id}); r.s("code") != "avatar_taken" {
		t.Fatalf("admin's picture taken: %v", r)
	}
	// The admin moves to a stock picture: theirs goes to the pool, the stock one is held.
	handshake(t, srv, msg{"t": "profile", "auth": ts, "op": "avatar", "avatar": "av01"})
	if r := handshake(t, srv, msg{"t": "profile", "auth": tb, "op": "avatar", "avatar": id}); r.s("avatar") != id {
		t.Fatalf("released picture: %v", r)
	}
	if r := handshake(t, srv, msg{"t": "profile", "auth": tb, "op": "avatar", "avatar": "av01"}); r.s("code") != "avatar_taken" {
		t.Fatalf("stock picture worn by an admin: %v", r)
	}
}
