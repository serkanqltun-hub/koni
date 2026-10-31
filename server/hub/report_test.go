package hub_test

import (
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/serkanqltun-hub/koni/server/hub"
	"github.com/serkanqltun-hub/koni/server/store"
)

func TestBugReport(t *testing.T) {
	dir := t.TempDir()
	errCode := func(m msg) string {
		t.Helper()
		if m.s("t") != "error" {
			t.Fatalf("expected error, got %v", m)
		}
		return m.s("code")
	}

	// off: no -reports and no -db
	off := httptest.NewServer(hub.New(hub.Config{}).Handler())
	defer off.Close()
	if c := errCode(handshake(t, off, msg{"t": "bug_report", "username": "Serkan", "text": "x"})); c != "no_reports" {
		t.Fatalf("off: %s", c)
	}

	// open server: the username names the file; 5 reports per IP per hour
	open := httptest.NewServer(hub.New(hub.Config{Reports: dir}).Handler())
	defer open.Close()
	big := strings.Repeat("a", 200<<10+1) // over 8 KiB: the handshake read limit is raised
	if c := errCode(handshake(t, open, msg{"t": "bug_report", "username": "Serkan", "text": big})); c != "too_large" {
		t.Fatalf("big: %s", c)
	}
	ok := handshake(t, open, msg{"t": "bug_report", "username": "Şükrü Ç", "version": "0.0.9", "text": "satır 1\nsatır 2"})
	if ok.s("t") != "bug_report_ok" || !strings.HasSuffix(ok.s("id"), "-sukruc.txt") {
		t.Fatalf("ok: %v", ok)
	}
	body, err := os.ReadFile(filepath.Join(dir, ok.s("id")))
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"Kullanıcı: Şükrü Ç\n", "Sürüm: 0.0.9\n", "IP: 127.0.0.1\n", "\n\nsatır 1\nsatır 2"} {
		if !strings.Contains(string(body), want) {
			t.Fatalf("missing %q in %q", want, body)
		}
	}
	for i := 0; i < 4; i++ {
		if r := handshake(t, open, msg{"t": "bug_report", "username": "Serkan", "text": "x"}); r.s("t") != "bug_report_ok" {
			t.Fatalf("report %d: %v", i+2, r)
		}
	}
	if c := errCode(handshake(t, open, msg{"t": "bug_report", "username": "Serkan", "text": "x"})); c != "rate_limited" {
		t.Fatalf("6th: %s", c)
	}
	if files, _ := os.ReadDir(dir); len(files) != 5 {
		t.Fatalf("want 5 files (same-second names get -2, -3…), got %d", len(files))
	}

	// account server: the session token names the reporter
	st, err := store.Open(filepath.Join(t.TempDir(), "koni.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	acc := httptest.NewServer(hub.New(hub.Config{Store: st, Reports: t.TempDir()}).Handler())
	defer acc.Close()
	if c := errCode(handshake(t, acc, msg{"t": "bug_report", "username": "Serkan", "text": "x"})); c != "auth_required" {
		t.Fatalf("no token: %s", c)
	}
	if c := errCode(handshake(t, acc, msg{"t": "bug_report", "auth": "nope", "text": "x"})); c != "bad_token" {
		t.Fatalf("bad token: %s", c)
	}
	code, _ := st.CreateInvite(0, time.Hour)
	tok := handshake(t, acc, msg{"t": "register", "code": code, "username": "Serkan", "password": "uzun-sifre"}).s("token")
	if r := handshake(t, acc, msg{"t": "bug_report", "auth": tok, "username": "Sahte", "text": "x"}); r.s("t") != "bug_report_ok" || !strings.HasSuffix(r.s("id"), "-serkan.txt") {
		t.Fatalf("account: %v", r)
	}
}
