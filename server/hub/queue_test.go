package hub_test

import (
	"net/http/httptest"
	"testing"
	"time"

	"github.com/serkanqltun-hub/koni/server/hub"
)

func queueSrcs(t *testing.T, m msg) []string {
	var out []string
	for _, it := range items(t, m) {
		out = append(out, it["media"].(map[string]any)["src"].(string))
	}
	return out
}

func mediaSrc(m msg) string {
	md, _ := m["media"].(map[string]any)
	s, _ := md["src"].(string)
	return s
}

// The first song starts when nothing plays; votes reorder; the first track_end for
// the current media advances once, later reports of the same end are ignored.
func TestQueueVoteAndAdvance(t *testing.T) {
	srv := httptest.NewServer(hub.New(hub.Config{MinLead: 30 * time.Millisecond, QueueStart: 100 * time.Millisecond}).Handler())
	defer srv.Close()
	a := dial(t, srv)
	a.send(msg{"t": "create_room", "name": "Lounge", "username": "Ayse"})
	created := a.waitType("room_created")
	b := dial(t, srv)
	b.send(msg{"t": "join", "invite": created.s("invite"), "username": "Berk"})
	b.waitType("joined")

	const s1, s2, s3 = "https://youtu.be/one", "https://youtu.be/two", "https://youtu.be/three"
	add := func(c *client, src string) {
		c.send(msg{"t": "queue_add", "media": msg{"kind": "url", "src": src, "title": src}})
	}
	add(a, s1)
	b.wait("first song becomes room media", func(m msg) bool { return m.s("t") == "state" && mediaSrc(m) == s1 })

	add(b, s2)
	a.wait("s2 queued", func(m msg) bool { return m.s("t") == "queue" && len(items(t, m)) == 1 }) // two connections: order the adds
	add(a, s3)
	add(a, s3)
	a.wait("duplicate refused", func(m msg) bool { return m.s("t") == "fun_error" })
	q := b.wait("two queued", func(m msg) bool { return m.s("t") == "queue" && len(items(t, m)) == 2 })
	if got := queueSrcs(t, q); got[0] != s2 || got[1] != s3 {
		t.Fatalf("add order: %v", got)
	}
	id3 := items(t, q)[1]["id"]
	b.send(msg{"t": "queue_vote", "queueId": id3})
	q = a.wait("vote reorders", func(m msg) bool { return m.s("t") == "queue" && queueSrcs(t, m)[0] == s3 })
	if v := items(t, q)[0]["voters"].([]any); len(v) != 1 {
		t.Fatalf("voters: %v", v)
	}

	a.send(msg{"t": "track_end", "src": s1})
	b.send(msg{"t": "track_end", "src": s1})
	b.wait("voted song plays next", func(m msg) bool { return m.s("t") == "state" && mediaSrc(m) == s3 })
	q = b.wait("queue shrinks", func(m msg) bool { return m.s("t") == "queue" })
	if got := queueSrcs(t, q); len(got) != 1 || got[0] != s2 {
		t.Fatalf("after advance: %v", got)
	}
	// the duplicate end report must not skip s3; the delayed auto-start plays it
	b.wait("auto-start", func(m msg) bool { return m.s("t") == "state" && m.s("mode") == "countdown" })
	b.send(msg{"t": "ping", "t0": 1})
	b.wait("no further media switch", func(m msg) bool {
		if m.s("t") == "state" && mediaSrc(m) != s3 {
			t.Fatalf("skipped to %s", mediaSrc(m))
		}
		return m.s("t") == "pong"
	})
}
