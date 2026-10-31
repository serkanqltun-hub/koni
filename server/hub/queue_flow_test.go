package hub_test

import (
	"net/http/httptest"
	"testing"
	"time"

	"github.com/serkanqltun-hub/koni/server/hub"
)

// loungePair: a room with two members and a fast queue start.
func loungePair(t *testing.T) (a, b *client) {
	t.Helper()
	srv := httptest.NewServer(hub.New(hub.Config{MinLead: 30 * time.Millisecond, QueueStart: 50 * time.Millisecond}).Handler())
	t.Cleanup(srv.Close)
	a = dial(t, srv)
	a.send(msg{"t": "create_room", "name": "Lounge", "username": "Ayse"})
	created := a.waitType("room_created")
	b = dial(t, srv)
	b.send(msg{"t": "join", "invite": created.s("invite"), "username": "Berk"})
	b.waitType("joined")
	return a, b
}

func queueAddMsg(src string) msg {
	return msg{"t": "queue_add", "media": msg{"kind": "url", "src": src, "title": src}}
}

func waitMedia(c *client, src string) msg {
	return c.wait("media "+src, func(m msg) bool { return m.s("t") == "state" && m.s("cause") == "media" && mediaSrc(m) == src })
}

// A double "next" (two members, or a double click) skips one song, not two.
func TestQueueNextOnce(t *testing.T) {
	a, b := loungePair(t)
	const s1, s2, s3 = "https://youtu.be/one", "https://youtu.be/two", "https://youtu.be/three"
	a.send(queueAddMsg(s1))
	waitMedia(b, s1)
	a.send(queueAddMsg(s2))
	a.wait("s2 queued", func(m msg) bool { return m.s("t") == "queue" && len(items(t, m)) == 1 })
	a.send(queueAddMsg(s3))
	a.wait("s3 queued", func(m msg) bool { return m.s("t") == "queue" && len(items(t, m)) == 2 })

	a.send(msg{"t": "queue_next", "src": s1})
	b.send(msg{"t": "queue_next", "src": s1}) // saw s1 too: must not skip s2
	waitMedia(b, s2)
	b.send(msg{"t": "ping", "t0": 1})
	b.wait("no second skip", func(m msg) bool {
		if m.s("t") == "state" && m.s("cause") == "media" {
			t.Fatalf("skipped again to %s", mediaSrc(m))
		}
		return m.s("t") == "pong"
	})
}

// The last song's end pauses the room; "play" then replays it from the start, and a
// song added later starts at once.
func TestQueueEndPausesAndReplays(t *testing.T) {
	a, b := loungePair(t)
	const s1, s2 = "https://youtu.be/one", "https://youtu.be/two"
	a.send(queueAddMsg(s1))
	waitMedia(b, s1)
	b.waitState("countdown", "play")
	a.send(msg{"t": "track_end", "src": s1})
	st := b.waitState("paused", "pause")
	if st["by"] != nil {
		t.Fatalf("server pause should have no by: %v", st)
	}
	a.send(msg{"t": "cmd", "cid": "p", "op": "play"})
	st = b.waitState("countdown", "seek")
	if st.f("anchorPos") != 0 {
		t.Fatalf("replay should start at 0: %v", st)
	}
	a.send(msg{"t": "track_end", "src": s1})
	b.waitState("paused", "pause")
	a.send(queueAddMsg(s2))
	waitMedia(b, s2)
}

// The same song queued again starts with a "seek" (players still hold it at its end).
func TestQueueSameSongAgain(t *testing.T) {
	a, b := loungePair(t)
	const s1 = "https://youtu.be/one"
	a.send(queueAddMsg(s1))
	waitMedia(b, s1)
	a.send(msg{"t": "track_end", "src": s1})
	b.waitState("paused", "pause")
	b.send(queueAddMsg(s1))
	waitMedia(a, s1)
	if st := a.wait("start", func(m msg) bool { return m.s("t") == "state" && m.s("mode") == "countdown" }); st.s("cause") != "seek" {
		t.Fatalf("same song should restart with seek: %v", st)
	}
}

// A song one member cannot open: that member is detached; when the one who queued it
// cannot open it either, it is skipped.
func TestQueueFailedSong(t *testing.T) {
	a, b := loungePair(t)
	const s1, s2 = "https://youtu.be/one", "https://youtu.be/two"
	a.send(queueAddMsg(s1)) // a queued s1
	waitMedia(b, s1)
	b.send(queueAddMsg(s2))
	b.wait("s2 queued", func(m msg) bool { return m.s("t") == "queue" && len(items(t, m)) == 1 })

	b.send(msg{"t": "track_end", "src": s1, "failed": true}) // not the one who queued it
	b.waitType("detached")
	b.send(msg{"t": "ping", "t0": 1})
	b.wait("not skipped", func(m msg) bool {
		if m.s("t") == "state" && m.s("cause") == "media" {
			t.Fatalf("one member's failure skipped the song")
		}
		return m.s("t") == "pong"
	})
	a.send(msg{"t": "track_end", "src": s1, "failed": true})
	waitMedia(b, s2)
}

// A personal-source film (kind "debrid") that most members cannot open (no provider)
// is not taken away from the one who can: they are detached, the film stays.
func TestDebridFailedDetachesOnly(t *testing.T) {
	srv := httptest.NewServer(hub.New(hub.Config{MinLead: 30 * time.Millisecond}).Handler())
	t.Cleanup(srv.Close)
	a := dial(t, srv)
	a.send(msg{"t": "create_room", "name": "Sinema", "username": "Ayse"})
	invite := a.waitType("room_created").s("invite")
	b, c := dial(t, srv), dial(t, srv)
	b.send(msg{"t": "join", "invite": invite, "username": "Berk"})
	b.waitType("joined")
	c.send(msg{"t": "join", "invite": invite, "username": "Cem"})
	c.waitType("joined")

	const hash = "0123456789abcdef0123456789abcdef01234567"
	a.send(msg{"t": "cmd", "cid": "m", "op": "media", "media": msg{"kind": "debrid", "src": hash, "title": "Film"}})
	waitMedia(b, hash)
	waitMedia(c, hash)
	b.send(msg{"t": "track_end", "src": hash, "failed": true})
	b.waitType("detached")
	c.send(msg{"t": "track_end", "src": hash, "failed": true})
	c.waitType("detached")
	a.send(msg{"t": "ping", "t0": 1})
	a.wait("film stays", func(m msg) bool {
		if m.s("t") == "state" && m.s("cause") == "pause" {
			t.Fatalf("members without a provider took the film away: %v", m)
		}
		return m.s("t") == "pong"
	})
}
