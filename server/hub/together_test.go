package hub

import "testing"

func media(src string) *Media { return &Media{Kind: "url", Src: src, Title: src} }

// cmd sends without the 3 cmd/s limit getting in the test's way.
func cmd(s *Session, in Inbound) {
	s.Room.call(func() { s.Member.cmdTimes = nil })
	in.T = "cmd"
	s.Handle(in)
}

// breakEnd reads break messages until the break ends.
func breakEnd(t *testing.T, s *Session) map[string]any {
	t.Helper()
	for {
		if m := next(t, s, "break"); m["end"] == true {
			return m
		}
	}
}

// Mola: the room pauses; when the last connected member says ready it starts with
// the "Hazır mısınız?" countdown.
func TestBreakReadyCountdown(t *testing.T) {
	_, a, b := pair(t)
	cmd(a, Inbound{Cid: "x", Op: "break"})
	if d := next(t, a, "denied"); d["reason"] != "no_media" {
		t.Fatalf("break without media: %v", d)
	}
	cmd(a, Inbound{Cid: "m1", Op: "media", Media: media("https://example.com/a.mp4")})
	cmd(a, Inbound{Cid: "p1", Op: "play", Pos: fp(0)})
	cmd(a, Inbound{Cid: "b1", Op: "break", Min: 99})
	br := next(t, b, "break")
	if br["min"] != float64(breakMaxMin) || br["by"] != "Serkan" || len(br["waiting"].([]any)) != 2 {
		t.Fatalf("break: %v", br)
	}
	next(t, a, "break")
	cmd(b, Inbound{Cid: "r1", Op: "ready"})
	if br := next(t, a, "break"); len(br["ready"].([]any)) != 1 || br["waiting"].([]any)[0] != a.Member.ID {
		t.Fatalf("one ready: %v", br)
	}
	cmd(a, Inbound{Cid: "r2", Op: "ready"})
	if end := breakEnd(t, b); end["cause"] != "ready" {
		t.Fatalf("break end: %v", end)
	}
	st := next(t, b, "state")
	if st["mode"] != ModeCountdown || st["cause"] != "release" || st["startAt"].(float64)-st["serverTs"].(float64) < 2500 {
		t.Fatalf("after break: %v", st)
	}
	cmd(b, Inbound{Cid: "r3", Op: "ready"})
	if d := next(t, b, "denied"); d["reason"] != "no_break" {
		t.Fatalf("ready without break: %v", d)
	}
}

// A member who leaves during a break is not waited for; play ends the break.
func TestBreakLeaveAndPlay(t *testing.T) {
	_, a, b := pair(t)
	cmd(a, Inbound{Cid: "m1", Op: "media", Media: media("https://example.com/a.mp4")})
	cmd(a, Inbound{Cid: "b1", Op: "break", Min: 5})
	next(t, b, "break")
	cmd(a, Inbound{Cid: "r1", Op: "ready"})
	next(t, b, "break")
	cmd(a, Inbound{Cid: "b2", Op: "break", Min: 5})
	next(t, b, "break")
	cmd(b, Inbound{Cid: "p1", Op: "play"})
	if end := breakEnd(t, a); end["cause"] != "play" {
		t.Fatalf("play ends the break: %v", end)
	}
	cmd(a, Inbound{Cid: "b3", Op: "break", Min: 5})
	next(t, a, "break")
	cmd(a, Inbound{Cid: "r2", Op: "ready"})
	next(t, a, "break")
	b.Handle(Inbound{T: "leave"})
	breakEnd(t, a)
}

// History: newest first, where the room left each media, no duplicates.
func TestRoomHistory(t *testing.T) {
	h, a, b := pair(t)
	cmd(a, Inbound{Cid: "m1", Op: "media", Media: media("https://example.com/a.mp4")})
	if h := next(t, b, "history"); len(h["items"].([]any)) != 1 {
		t.Fatalf("history: %v", h)
	}
	cmd(a, Inbound{Cid: "s1", Op: "seek", Pos: fp(100)})
	cmd(a, Inbound{Cid: "p1", Op: "pause", Pos: fp(120)})
	cmd(b, Inbound{Cid: "m2", Op: "media", Media: media("https://example.com/b.mp4")})
	hi := next(t, b, "history")
	items := hi["items"].([]any)
	first, second := items[0].(map[string]any), items[1].(map[string]any)
	if len(items) != 2 || first["by"] != "Coni" || second["pos"] != 120.0 || second["media"].(map[string]any)["src"] != "https://example.com/a.mp4" {
		t.Fatalf("history: %v", hi)
	}
	cmd(a, Inbound{Cid: "m3", Op: "media", Media: media("https://example.com/a.mp4")})
	hi = next(t, b, "history")
	if items := hi["items"].([]any); len(items) != 2 || items[0].(map[string]any)["by"] != "Serkan" {
		t.Fatalf("reopened: %v", hi)
	}
	c, code := h.JoinRoom(a.Invite, "Cem", "")
	if code != "" {
		t.Fatal(code)
	}
	if len(c.Snap.History) != 2 {
		t.Fatalf("snapshot history: %v", c.Snap.History)
	}
}
