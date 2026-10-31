package hub

import (
	"encoding/json"
	"testing"
	"time"
)

// collect drains s until a pong arrives and returns the messages of type typ before it.
func collect(t *testing.T, s *Session, typ string) []map[string]any {
	t.Helper()
	s.Handle(Inbound{T: "ping", T0: 1})
	var got []map[string]any
	timeout := time.After(2 * time.Second)
	for {
		select {
		case b := <-s.Out():
			var m map[string]any
			if err := json.Unmarshal(b, &m); err != nil {
				t.Fatal(err)
			}
			switch m["t"] {
			case "pong":
				return got
			case typ:
				got = append(got, m)
			}
		case <-timeout:
			t.Fatal("timeout waiting for pong")
		}
	}
}

func TestReactAllowListAndRate(t *testing.T) {
	_, a, b := pair(t)
	collect(t, a, "") // skip join chatter
	b.Handle(Inbound{T: "react", Emoji: "😂"})
	b.Handle(Inbound{T: "react", Emoji: "🔥"}) // inside 700 ms: dropped
	b.Handle(Inbound{T: "react", Emoji: "💩"}) // not allowed: dropped
	r := next(t, a, "reaction")
	if r["user"] != b.Member.ID || r["name"] != "Coni" || r["emoji"] != "😂" {
		t.Fatalf("%v", r)
	}
	if got := collect(t, b, "reaction"); len(got) != 1 || got[0]["emoji"] != "😂" {
		t.Fatalf("sender sees its own reaction once: %v", got)
	}
	if got := collect(t, a, "reaction"); len(got) != 0 {
		t.Fatalf("extra reactions: %v", got)
	}
	time.Sleep(reactGap)
	b.Handle(Inbound{T: "react", Emoji: "🔥"})
	if r := next(t, a, "reaction"); r["emoji"] != "🔥" {
		t.Fatalf("%v", r)
	}
	if got := collect(t, a, "activity"); len(got) != 0 {
		t.Fatalf("reactions stay out of the feed: %v", got)
	}
}

// The first start of a media gets the ReadyLead countdown; later starts the normal lead.
func TestReadyLeadOnFirstStart(t *testing.T) {
	h := New(Config{MinLead: 20 * time.Millisecond, ReadyLead: 300 * time.Millisecond})
	a, _ := h.CreateRoom("Test", "Serkan", "", "")
	lead := func(st map[string]any) float64 { return st["startAt"].(float64) - st["serverTs"].(float64) }

	a.Handle(Inbound{T: "cmd", Cid: "p0", Op: "play"}) // no media yet: normal lead
	if st := next(t, a, "state"); st["mode"] != "countdown" || lead(st) >= 300 {
		t.Fatalf("%v", st)
	}
	a.Handle(Inbound{T: "cmd", Cid: "m", Op: "media", Media: &Media{Kind: "url", Src: "https://youtu.be/x"}})
	for st := next(t, a, "state"); st["cause"] != "media"; st = next(t, a, "state") {
	}
	a.Handle(Inbound{T: "cmd", Cid: "p1", Op: "play"})
	if st := next(t, a, "state"); st["mode"] != "countdown" || lead(st) < 300 {
		t.Fatalf("first start: %v", st)
	}
	// a stall before PLAYING keeps the media unstarted: the release waits again
	a.Handle(Inbound{T: "report", State: "stalled"})
	if st := next(t, a, "state"); st["mode"] != "hold" {
		t.Fatalf("%v", st)
	}
	a.Handle(Inbound{T: "report", State: "ready", BufferSec: 12})
	if st := next(t, a, "state"); st["cause"] != "release" || lead(st) < 300 {
		t.Fatalf("release before first start: %v", st)
	}
	if st := next(t, a, "state"); st["mode"] != "playing" {
		t.Fatalf("%v", st)
	}
	time.Sleep(time.Second) // cmd rate limit: 3 per second
	a.Handle(Inbound{T: "cmd", Cid: "p2", Op: "pause"})
	next(t, a, "state")
	a.Handle(Inbound{T: "cmd", Cid: "p3", Op: "play"})
	if st := next(t, a, "state"); st["mode"] != "countdown" || lead(st) >= 300 {
		t.Fatalf("second start: %v", st)
	}
}

func TestMemberStatusRtt(t *testing.T) {
	_, a, b := pair(t)
	b.Handle(Inbound{T: "report", State: "paused", Pos: fp(0)})
	if st := next(t, a, "member_status"); st["rttMs"] != nil {
		t.Fatalf("unknown rtt must be absent: %v", st)
	}
	b.Handle(Inbound{T: "ping", T0: 1, RttMs: 42})
	time.Sleep(statusInterval)
	b.Handle(Inbound{T: "report", State: "paused", Pos: fp(0)})
	if st := next(t, a, "member_status"); st["rttMs"] != float64(42) {
		t.Fatalf("%v", st)
	}
}
