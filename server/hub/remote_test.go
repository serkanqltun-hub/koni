package hub_test

import (
	"io"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/serkanqltun-hub/koni/server/hub"
	"github.com/serkanqltun-hub/koni/server/store"
)

// p0.24: a phone page on /ws drives the same account's desktop over its lobby.
func TestPhoneRemote(t *testing.T) {
	st, err := store.Open(filepath.Join(t.TempDir(), "koni.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	srv := httptest.NewServer(hub.New(hub.Config{Store: st}).Handler())
	defer srv.Close()

	// A browser opening the address gets the page, not an upgrade error.
	res, err := http.Get(srv.URL + "/ws")
	if err != nil {
		t.Fatal(err)
	}
	page, _ := io.ReadAll(res.Body)
	res.Body.Close()
	if res.StatusCode != 200 || !strings.Contains(string(page), "remote_state") {
		t.Fatalf("page: %d", res.StatusCode)
	}

	code, _ := st.CreateInvite(0, time.Hour)
	tok := handshake(t, srv, msg{"t": "register", "code": code, "username": "Serkan", "password": "uzun-sifre"}).s("token")
	if r := handshake(t, srv, msg{"t": "remote", "auth": "yanlis"}); r.s("code") != "bad_token" {
		t.Fatalf("bad token: %v", r)
	}

	phone := dial(t, srv)
	phone.send(msg{"t": "remote", "auth": tok})
	if r := phone.next(); r.s("t") != "remote" || !strings.HasPrefix(r.s("name"), "Serkan#") {
		t.Fatalf("remote hello: %v", r)
	}
	// Desktop app not open: the phone hears so.
	phone.send(msg{"t": "remote", "op": "toggle"})
	if r := phone.waitType("remote_state"); r["online"] != false {
		t.Fatalf("offline: %v", r)
	}

	desk := dial(t, srv)
	desk.send(msg{"t": "hello", "auth": tok})
	desk.waitType("hello")
	phone.send(msg{"t": "remote", "op": "seek", "by": 9999})
	phone.send(msg{"t": "remote", "op": "rm -rf"}) // unknown ops are dropped
	if r := desk.waitType("remote"); r.s("op") != "seek" || r.f("by") != 600 {
		t.Fatalf("forwarded: %v", r)
	}
	desk.send(msg{"t": "remote_state", "title": "Film", "paused": true, "pos": 12.5, "dur": 100, "vol": 80})
	r := phone.wait("online state", func(m msg) bool { return m.s("t") == "remote_state" && m["online"] == true })
	if r.s("title") != "Film" || r["paused"] != true || r.f("pos") != 12.5 || r.f("vol") != 80 {
		t.Fatalf("state: %v", r)
	}
}
