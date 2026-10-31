package hub

import (
	"encoding/json"
	"strings"
	"testing"
	"time"
)

func newTestHub() *Hub {
	return New(Config{MinLead: 20 * time.Millisecond, GhostWait: 50 * time.Millisecond, EmptyWait: 100 * time.Millisecond})
}

// next reads messages from s until one with type t arrives.
func next(t *testing.T, s *Session, typ string) map[string]any {
	t.Helper()
	timeout := time.After(2 * time.Second)
	for {
		select {
		case b, ok := <-s.Out():
			if !ok {
				t.Fatalf("channel closed waiting for %s", typ)
			}
			var m map[string]any
			if err := json.Unmarshal(b, &m); err != nil {
				t.Fatal(err)
			}
			if m["t"] == typ {
				return m
			}
		case <-timeout:
			t.Fatalf("timeout waiting for %s", typ)
		}
	}
}

func pair(t *testing.T) (*Hub, *Session, *Session) {
	t.Helper()
	h := newTestHub()
	a, code := h.CreateRoom("Test", "Serkan", "", "")
	if code != "" {
		t.Fatal(code)
	}
	b, code := h.JoinRoom(a.Invite, "Coni", "")
	if code != "" {
		t.Fatal(code)
	}
	return h, a, b
}

func fp(f float64) *float64 { return &f }

func TestJoinValidation(t *testing.T) {
	h, a, _ := pair(t)
	if _, c := h.JoinRoom(a.Invite, "coni", ""); c != "name_taken" {
		t.Fatalf("got %q", c)
	}
	if _, c := h.JoinRoom(a.Invite, "  ", ""); c != "bad_username" {
		t.Fatalf("got %q", c)
	}
	if _, c := h.JoinRoom("nope", "x", ""); c != "bad_invite" {
		t.Fatalf("got %q", c)
	}
	for i := 0; i < MaxMembers; i++ {
		h.JoinRoom(a.Invite, string(rune('A'+i))+"x", "")
	}
	if _, c := h.JoinRoom(a.Invite, "Late", ""); c != "room_full" {
		t.Fatalf("got %q", c)
	}
}

func TestPlayCountdownThenPlaying(t *testing.T) {
	_, a, b := pair(t)
	a.Handle(Inbound{T: "cmd", Cid: "c1", Op: "play", Pos: fp(100)})
	sa := next(t, a, "state")
	sb := next(t, b, "state")
	if sa["mode"] != "countdown" || sa["ack"] != "c1" || sb["ack"] != nil {
		t.Fatalf("a=%v b=%v", sa, sb)
	}
	if sa["startAt"].(float64) <= sa["anchorTs"].(float64) {
		t.Fatal("startAt must follow anchorTs")
	}
	st := next(t, b, "state")
	if st["mode"] != "playing" || st["cause"] != "start" || st["anchorPos"].(float64) != 100 {
		t.Fatalf("%v", st)
	}
	if st["seq"].(float64) <= sb["seq"].(float64) {
		t.Fatal("seq must increase")
	}
}

func TestPauseUsesClientPositionAndReportsWho(t *testing.T) {
	_, a, b := pair(t)
	a.Handle(Inbound{T: "cmd", Cid: "c1", Op: "play", Pos: fp(10)})
	next(t, b, "state")
	next(t, b, "state") // playing
	next(t, a, "state")
	next(t, a, "state")
	b.Handle(Inbound{T: "cmd", Cid: "c2", Op: "pause", Pos: fp(12.5)})
	st := next(t, a, "state")
	if st["mode"] != "paused" || st["anchorPos"].(float64) != 12.5 || st["by"].(map[string]any)["name"] != "Coni" {
		t.Fatalf("%v", st)
	}
	act := next(t, a, "activity")
	for act["kind"] != "pause" {
		act = next(t, a, "activity")
	}
	if act["text"] != "Coni durdurdu: 00:13" && act["text"] != "Coni durdurdu: 00:12" {
		t.Fatalf("%v", act["text"])
	}
}

func TestHostOnly(t *testing.T) {
	_, a, b := pair(t)
	a.Handle(Inbound{T: "cmd", Cid: "p", Op: "set_policy", Policy: PolicyHostOnly})
	next(t, b, "policy")
	b.Handle(Inbound{T: "cmd", Cid: "x", Op: "pause"})
	if d := next(t, b, "denied"); d["reason"] != "host_only" || d["ack"] != "x" {
		t.Fatalf("%v", d)
	}
	b.Handle(Inbound{T: "cmd", Cid: "y", Op: "set_policy", Policy: PolicyFree})
	if d := next(t, b, "denied"); d["reason"] != "not_host" {
		t.Fatalf("%v", d)
	}
	a.Handle(Inbound{T: "cmd", Cid: "z", Op: "pause"})
	if st := next(t, b, "state"); st["mode"] != "paused" {
		t.Fatalf("%v", st)
	}
}

func TestCoHost(t *testing.T) {
	h, a, b := pair(t)
	c, code := h.JoinRoom(a.Invite, "Furkan", "")
	if code != "" {
		t.Fatal(code)
	}
	a.Handle(Inbound{T: "cmd", Cid: "p", Op: "set_policy", Policy: PolicyHostOnly})
	next(t, c, "policy")
	b.Handle(Inbound{T: "cmd", Cid: "x", Op: "set_cohost", User: c.Member.ID, On: true})
	if d := next(t, b, "denied"); d["reason"] != "not_host" {
		t.Fatalf("%v", d)
	}
	a.Handle(Inbound{T: "cmd", Cid: "y", Op: "set_cohost", User: c.Member.ID, On: true})
	if r := next(t, b, "member_role"); r["id"] != c.Member.ID || r["cohost"] != true {
		t.Fatalf("%v", r)
	}
	c.Handle(Inbound{T: "cmd", Cid: "z", Op: "pause"})
	if st := next(t, b, "state"); st["mode"] != "paused" {
		t.Fatalf("co-host must control in host_only: %v", st)
	}
	b.Handle(Inbound{T: "cmd", Cid: "w", Op: "pause"})
	if d := next(t, b, "denied"); d["reason"] != "host_only" {
		t.Fatalf("%v", d)
	}
	// The host leaves: the co-host wins over the earlier-joined member.
	a.Handle(Inbound{T: "leave"})
	if l := next(t, b, "member_left"); l["host"] != c.Member.ID {
		t.Fatalf("co-host should become host: %v", l)
	}
	if !c.Room.call(func() {}) || !c.Member.Host || c.Member.CoHost {
		t.Fatal("c should be plain host now")
	}
}

func TestMediaResetsRoomAndReachesLateJoiner(t *testing.T) {
	h, a, b := pair(t)
	a.Handle(Inbound{T: "cmd", Cid: "c1", Op: "play", Pos: fp(50)})
	next(t, b, "state")
	next(t, b, "state") // playing
	next(t, a, "state")
	next(t, a, "state")
	b.Handle(Inbound{T: "cmd", Cid: "m1", Op: "media", Media: &Media{Kind: "file", Src: `C:\film.mkv`}})
	if d := next(t, b, "denied"); d["reason"] != "bad_media" {
		t.Fatalf("local path must be rejected: %v", d)
	}
	b.Handle(Inbound{T: "cmd", Cid: "m2", Op: "media", Media: &Media{Kind: "url", Src: " https://youtu.be/x ", Duration: 212, Referer: "https://site.example/watch/1"}})
	st := next(t, a, "state")
	md, _ := st["media"].(map[string]any)
	if st["cause"] != "media" || st["mode"] != "paused" || st["anchorPos"].(float64) != 0 ||
		md["src"] != "https://youtu.be/x" || md["title"] != "https://youtu.be/x" || md["duration"].(float64) != 212 ||
		md["referer"] != "https://site.example/watch/1" {
		t.Fatalf("%v", st)
	}
	c, code := h.JoinRoom(a.Invite, "Late", "")
	if code != "" {
		t.Fatal(code)
	}
	if m := c.Snap.State.Media; m == nil || m.Kind != "url" || m.Src != "https://youtu.be/x" || m.Referer != "https://site.example/watch/1" {
		t.Fatalf("%+v", c.Snap.State)
	}
	if m, ok := cleanMedia(&Media{Kind: "url", Src: "https://cdn.example/a.m3u8", Referer: "javascript:alert(1)"}); !ok || m.Referer != "" {
		t.Fatalf("invalid referer must be dropped, media kept: %+v %v", m, ok)
	}
	subs := []Sub{{URL: "https://cdn.example/tr.vtt", Lang: "tr", Title: "Türkçe"}, {URL: "file:///etc/passwd"}}
	if m, _ := cleanMedia(&Media{Kind: "url", Src: "https://cdn.example/a.m3u8", Referer: "https://site.example/", Subs: subs}); len(m.Subs) != 1 || m.Subs[0].Lang != "tr" {
		t.Fatalf("subs: %+v", m.Subs)
	}
	if m, _ := cleanMedia(&Media{Kind: "url", Src: "https://youtu.be/x", Subs: subs}); m.Subs != nil {
		t.Fatalf("subs need a captured stream: %+v", m.Subs)
	}
}

func TestDebridMediaCarriesOnlyTheTorrent(t *testing.T) {
	_, a, b := pair(t)
	for _, bad := range []string{"https://real-debrid.com/d/ABC", "abc", strings.Repeat("g", 40)} {
		if _, ok := cleanMedia(&Media{Kind: "debrid", Src: bad}); ok {
			t.Fatalf("%q must be rejected", bad)
		}
	}
	hash := strings.Repeat("AB12", 10)
	idx := 3
	a.Handle(Inbound{T: "cmd", Cid: "m", Op: "media", Media: &Media{Kind: "debrid", Src: hash, Title: "Film (2020)",
		File: `Film.2020/Film.2020.1080p.mkv`, Idx: &idx}})
	md := next(t, b, "state")["media"].(map[string]any)
	if md["kind"] != "debrid" || md["src"] != strings.ToLower(hash) || md["file"] != "Film.2020.1080p.mkv" || md["idx"].(float64) != 3 {
		t.Fatalf("%v", md)
	}
	neg := -1
	if m, ok := cleanMedia(&Media{Kind: "debrid", Src: hash, Idx: &neg, File: strings.Repeat("x", 300)}); !ok || m.Idx != nil || m.File != "" {
		t.Fatalf("invalid hints must be dropped, media kept: %+v %v", m, ok)
	}
	if m, _ := cleanMedia(&Media{Kind: "url", Src: "https://youtu.be/x", File: "a.mkv", Idx: &idx}); m.File != "" || m.Idx != nil {
		t.Fatalf("torrent hints only for debrid: %+v", m)
	}
}

func TestPageMediaOpensBrowseView(t *testing.T) {
	_, a, b := pair(t)
	if _, ok := cleanMedia(&Media{Kind: "page", Src: "file:///etc/passwd"}); ok {
		t.Fatal("page must be http(s)")
	}
	a.Handle(Inbound{T: "cmd", Cid: "w", Op: "set_view", View: ViewWatch})
	next(t, b, "view")
	a.Handle(Inbound{T: "cmd", Cid: "p", Op: "media", Media: &Media{Kind: "page", Src: "https://site.example/watch/1", Title: "Belgesel",
		Referer: "https://x.example/"}})
	md := next(t, b, "state")["media"].(map[string]any)
	if md["kind"] != "page" || md["src"] != "https://site.example/watch/1" || md["referer"] != nil {
		t.Fatalf("%v", md)
	}
	if v := next(t, b, "view"); v["view"] != "browse" {
		t.Fatalf("view: %v", v)
	}
}

func TestCloseMediaEmptiesRoom(t *testing.T) {
	_, a, b := pair(t)
	a.Handle(Inbound{T: "cmd", Cid: "m", Op: "media", Media: &Media{Kind: "url", Src: "https://v.example/a.mp4", Title: "A"}})
	next(t, a, "state")
	next(t, b, "state")
	b.Handle(Inbound{T: "cmd", Cid: "x", Op: "close"})
	st := next(t, a, "state")
	if _, ok := st["media"]; ok || st["mode"] != "paused" {
		t.Fatalf("%v", st)
	}
	b.Handle(Inbound{T: "cmd", Cid: "y", Op: "close"})
	if e := next(t, b, "denied"); e["reason"] != "no_media" {
		t.Fatalf("%v", e)
	}
}

func TestStallHoldAndRelease(t *testing.T) {
	_, a, b := pair(t)
	a.Handle(Inbound{T: "cmd", Cid: "c1", Op: "play", Pos: fp(50)})
	next(t, a, "state")
	next(t, a, "state") // playing
	b.Handle(Inbound{T: "report", State: "stalled", Pos: fp(50.2)})
	st := next(t, a, "state")
	if st["mode"] != "hold" || st["waitingFor"].([]any)[0] != b.Member.ID {
		t.Fatalf("%v", st)
	}
	// still waiting: refilling reports show buffer status but don't release
	b.Handle(Inbound{T: "report", State: "refilling", BufferSec: 6})
	bs := next(t, a, "buffer_status")
	for bs["bufferPct"].(float64) != 50 {
		bs = next(t, a, "buffer_status")
	}
	b.Handle(Inbound{T: "report", State: "ready", BufferSec: 12})
	rel := next(t, a, "state")
	if rel["mode"] != "countdown" || rel["cause"] != "release" {
		t.Fatalf("%v", rel)
	}
	if pl := next(t, a, "state"); pl["mode"] != "playing" {
		t.Fatalf("%v", pl)
	}
}

func TestHoldReleasedWhenWaitingMemberLeaves(t *testing.T) {
	_, a, b := pair(t)
	a.Handle(Inbound{T: "cmd", Cid: "c1", Op: "play", Pos: fp(1)})
	next(t, a, "state")
	next(t, a, "state")
	b.Handle(Inbound{T: "report", State: "stalled"})
	next(t, a, "state")
	b.Close()
	if st := next(t, a, "state"); st["cause"] != "release" {
		t.Fatalf("%v", st)
	}
}

func TestSeekClearsHoldAndSkipActivity(t *testing.T) {
	_, a, b := pair(t)
	a.Handle(Inbound{T: "cmd", Cid: "c1", Op: "play", Pos: fp(1)})
	next(t, a, "state")
	next(t, a, "state")
	b.Handle(Inbound{T: "report", State: "stalled"})
	next(t, a, "state")
	a.Handle(Inbound{T: "cmd", Cid: "s", Op: "seek", Pos: fp(300)})
	st := next(t, a, "state")
	if st["mode"] != "countdown" || st["cause"] != "seek" || st["anchorPos"].(float64) != 300 {
		t.Fatalf("%v", st)
	}
	b.Handle(Inbound{T: "report", State: "playing", SkippedSec: 42})
	for {
		act := next(t, a, "activity")
		if act["kind"] == "skip" {
			if act["text"] != "Coni 42 s ileri atladı" {
				t.Fatalf("%v", act["text"])
			}
			return
		}
	}
}

func TestRateLimit(t *testing.T) {
	_, a, _ := pair(t)
	for i := 0; i < 3; i++ {
		a.Handle(Inbound{T: "cmd", Cid: "ok", Op: "pause"})
	}
	a.Handle(Inbound{T: "cmd", Cid: "over", Op: "pause"})
	if d := next(t, a, "denied"); d["reason"] != "rate_limited" || d["ack"] != "over" {
		t.Fatalf("%v", d)
	}
}

func TestPingAndHostHandover(t *testing.T) {
	_, a, b := pair(t)
	b.Handle(Inbound{T: "ping", T0: 123, RttMs: 400})
	p := next(t, b, "pong")
	if p["t0"].(float64) != 123 || p["serverTs"].(float64) == 0 {
		t.Fatalf("%v", p)
	}
	a.Close()
	if l := next(t, b, "member_left"); l["host"] != b.Member.ID {
		t.Fatalf("member_left must name the new host: %v", l)
	}
	if !b.Room.call(func() {}) || !b.Member.Host {
		t.Fatal("b should become host")
	}
}

func TestRoomRemovedWhenEmpty(t *testing.T) {
	h, a, b := pair(t)
	a.Close()
	b.Close()
	deadline := time.Now().Add(2 * time.Second)
	for h.RoomCount() != 0 {
		if time.Now().After(deadline) {
			t.Fatal("room not removed")
		}
		time.Sleep(10 * time.Millisecond)
	}
	if _, c := h.JoinRoom(a.Invite, "Late", ""); c != "bad_invite" {
		t.Fatalf("got %q", c)
	}
}

func TestResumeKeepsMemberAndHost(t *testing.T) {
	h, a, b := pair(t)
	a.Close() // connection drops: a becomes a ghost, stays host
	if away := next(t, b, "member_away"); away["id"] != a.Member.ID {
		t.Fatalf("%v", away)
	}
	if _, c := h.Resume(a.RoomID, a.Member.ID, "wrong"); c != "resume_failed" {
		t.Fatalf("got %q", c)
	}
	a2, c := h.Resume(a.RoomID, a.Member.ID, a.Token)
	if c != "" || !a2.Resumed || a2.Member != a.Member {
		t.Fatalf("resume: %q", c)
	}
	next(t, b, "member_back")
	a.Close() // stale connection closing again must not ghost the new one
	time.Sleep(100 * time.Millisecond)
	if !b.Room.call(func() {}) || !a.Member.Host || a.Member.ghost || a.Member.closed {
		t.Fatal("resumed member should stay host and connected")
	}
	a2.Handle(Inbound{T: "cmd", Cid: "p", Op: "play"})
	next(t, a2, "state")
}

func TestChronicStallOfferAndContinueWithout(t *testing.T) {
	_, a, b := pair(t)
	// stalls while a media is still opening (the first seconds) are not counted
	a.Handle(Inbound{T: "cmd", Cid: "p", Op: "play"})
	b.Handle(Inbound{T: "report", State: "stalled", Pos: fp(0)})
	b.Handle(Inbound{T: "report", State: "ready", Pos: fp(0), BufferSec: 12})
	a.Handle(Inbound{T: "cmd", Cid: "s", Op: "seek", Pos: fp(30)})
	time.Sleep(400 * time.Millisecond)
	for range chronicStalls {
		a.Handle(Inbound{T: "cmd", Cid: "p", Op: "play"})
		b.Handle(Inbound{T: "report", State: "stalled", Pos: fp(0)})
		b.Handle(Inbound{T: "report", State: "ready", Pos: fp(0), BufferSec: 12})
		time.Sleep(400 * time.Millisecond) // stay under the rate limit
	}
	if o := next(t, a, "detach_offer"); o["user"] != b.Member.ID {
		t.Fatalf("%v", o)
	}
	b.Handle(Inbound{T: "report", State: "stalled", Pos: fp(0)}) // room is playing again → HOLD
	a.Handle(Inbound{T: "cmd", Cid: "cw", Op: "continue_without", User: b.Member.ID})
	next(t, b, "detached")
	// the host's continue_without releases the hold (by = host; earlier releases had no by)
	for st := next(t, a, "state"); st["cause"] != "release" || st["by"] == nil; st = next(t, a, "state") {
	}
	// a detached member's stalls no longer hold the room
	b.Handle(Inbound{T: "report", State: "stalled", Pos: fp(0)})
	b.Handle(Inbound{T: "cmd", Cid: "rs", Op: "resync"})
	st := next(t, b, "state")
	for st["ack"] != "rs" {
		st = next(t, b, "state")
	}
	if st["mode"] == ModeHold {
		t.Fatalf("detached stall held the room: %v", st)
	}
	if !b.Room.call(func() {}) || b.Member.detached {
		t.Fatal("resync should clear detached")
	}
}

func TestDownloadBarrier(t *testing.T) {
	_, a, b := pair(t)
	a.Handle(Inbound{T: "cmd", Cid: "m", Op: "media", Media: &Media{Kind: "download", Src: "https://youtu.be/x", Task: "forged"}})
	st := next(t, b, "state")
	md := st["media"].(map[string]any)
	task, _ := md["task"].(string)
	if md["kind"] != "download" || task == "" || task == "forged" {
		t.Fatalf("server must assign the task: %v", md)
	}
	a.Handle(Inbound{T: "dl_progress", TaskID: task, State: DlReady})
	if s := next(t, b, "dl_status"); s["user"] != a.Member.ID || s["pct"].(float64) != 100 {
		t.Fatalf("%v", s)
	}
	// b has nothing loaded yet: its stalls must not hold anyone, stale tasks are ignored
	b.Handle(Inbound{T: "report", State: "stalled"})
	b.Handle(Inbound{T: "dl_progress", TaskID: "old", State: DlReady})
	b.Handle(Inbound{T: "dl_progress", TaskID: task, State: DlDownloading, Pct: 40})
	for {
		var m map[string]any
		if err := json.Unmarshal(<-a.Out(), &m); err != nil {
			t.Fatal(err)
		}
		if (m["t"] == "state" && m["mode"] == "hold") || (m["t"] == "dl_status" && m["taskId"] != task) {
			t.Fatalf("stall/stale task leaked: %v", m)
		}
		if m["t"] == "dl_status" && m["user"] == b.Member.ID {
			if m["pct"].(float64) != 40 {
				t.Fatalf("%v", m)
			}
			break
		}
	}
	b.Handle(Inbound{T: "dl_progress", TaskID: task, State: DlReady})
	st = next(t, a, "state")
	if st["mode"] != "countdown" || st["cause"] != "download_ready" {
		t.Fatalf("barrier should start the room: %v", st)
	}
	snapOK := a.Room.call(func() {
		if d := a.Room.snapshot().Downloads; len(d) != 2 || d[0].User != a.Member.ID {
			t.Errorf("snapshot downloads: %+v", d)
		}
	})
	if !snapOK {
		t.Fatal("room gone")
	}
}

func TestViewModes(t *testing.T) {
	h, a, b := pair(t)
	if a.Snap.View != ViewBrowse {
		t.Fatalf("new room starts in browse: %q", a.Snap.View)
	}
	b.Handle(Inbound{T: "cmd", Cid: "v1", Op: "set_view", View: "bogus"})
	if d := next(t, b, "denied"); d["reason"] != "bad_op" {
		t.Fatalf("%v", d)
	}
	b.Handle(Inbound{T: "cmd", Cid: "v2", Op: "set_view", View: ViewListen})
	if m := next(t, a, "view"); m["view"] != ViewListen || m["by"] != "Coni" {
		t.Fatalf("%v", m)
	}
	next(t, b, "view")
	b.Handle(Inbound{T: "cmd", Cid: "v3", Op: "set_view", View: ViewBrowse})
	next(t, a, "view")
	next(t, b, "view")
	// a film picked while browsing switches everyone to watch
	a.Handle(Inbound{T: "cmd", Cid: "m", Op: "media", Media: &Media{Kind: "url", Src: "https://youtu.be/x"}})
	if m := next(t, b, "view"); m["view"] != ViewWatch {
		t.Fatalf("%v", m)
	}
	c, _ := h.JoinRoom(a.Invite, "Late", "")
	if c.Snap.View != ViewWatch {
		t.Fatalf("late joiner view: %q", c.Snap.View)
	}
	a.Handle(Inbound{T: "cmd", Cid: "p", Op: "set_policy", Policy: PolicyHostOnly})
	c.Handle(Inbound{T: "cmd", Cid: "v4", Op: "set_view", View: ViewListen})
	if d := next(t, c, "denied"); d["reason"] != "host_only" {
		t.Fatalf("%v", d)
	}
}

func TestEmptyRoomRejoin(t *testing.T) {
	h := New(Config{MinLead: 20 * time.Millisecond, GhostWait: 20 * time.Millisecond, EmptyWait: 2 * time.Second})
	a, code := h.CreateRoom("Test", "Serkan", "", "")
	if code != "" {
		t.Fatal(code)
	}
	a.Handle(Inbound{T: "cmd", Cid: "p", Op: "play", Pos: fp(10)})
	next(t, a, "state")
	a.Close()
	deadline := time.Now().Add(2 * time.Second)
	for {
		n := -1
		a.Room.call(func() { n = len(a.Room.members) })
		if n == 0 {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("member did not leave")
		}
		time.Sleep(10 * time.Millisecond)
	}
	b, code := h.JoinRoom(a.Invite, "Serkan", "")
	if code != "" {
		t.Fatalf("rejoin empty room: %s", code)
	}
	if !b.Member.Host {
		t.Fatal("first one back should be host")
	}
	if b.Snap.State.Mode != ModePaused {
		t.Fatalf("empty room should be paused, got %s", b.Snap.State.Mode)
	}
}
