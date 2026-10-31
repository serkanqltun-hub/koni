package hub_test

import (
	"net/http/httptest"
	"testing"
	"time"

	"github.com/serkanqltun-hub/koni/server/discover"
	"github.com/serkanqltun-hub/koni/server/hub"
)

func waitNight(c *client, phase string) msg {
	return c.wait("night "+phase, func(m msg) bool { return m.s("t") == "night" && m.s("phase") == phase })
}

func TestNightFlow(t *testing.T) {
	d, err := discover.New("test-key", "")
	if err != nil {
		t.Fatal(err)
	}
	d.TMDBBase = fakeTMDB(t).URL
	srv := httptest.NewServer(hub.New(hub.Config{MinLead: 30 * time.Millisecond, Discover: d}).Handler())
	defer srv.Close()

	a := dial(t, srv)
	a.send(msg{"t": "create_room", "name": "Oda", "username": "Ayse"})
	created := a.waitType("room_created")
	b := dial(t, srv)
	b.send(msg{"t": "join", "invite": created.s("invite"), "username": "Berk"})
	b.waitType("joined")

	a.send(msg{"t": "night_start", "kind": "movie", "perUser": 2})
	n := waitNight(b, "setup")
	if n.s("kind") != "movie" || n.f("perUser") != 2 || n.s("by") != "Ayse" || len(n["members"].([]any)) != 2 {
		t.Fatalf("start: %v", n)
	}

	// setup → pick only when everyone is ready
	a.send(msg{"t": "night_ready", "ready": true, "mood": "Gerim gerim"})
	n = a.wait("a ready", func(m msg) bool {
		return m.s("t") == "night" && m["members"].([]any)[0].(map[string]any)["ready"] == true
	})
	if n.s("phase") != "setup" {
		t.Fatalf("advanced with one ready: %v", n)
	}
	b.send(msg{"t": "night_ready", "ready": true, "mood": "Özel filtre"})
	waitNight(a, "pick")

	a.send(msg{"t": "night_suggest", "filter": msg{"genres": []int{53}, "voteMin": 7}})
	if s := a.waitType("night_suggestions"); len(items(t, s)) != 12 || s.f("total") != 0 {
		t.Fatalf("suggestions: %v", s)
	}
	time.Sleep(1100 * time.Millisecond) // rate limit
	a.send(msg{"t": "night_search", "query": "matrix"})
	if r := a.waitType("night_results"); items(t, r)[0]["id"] != 500.0 {
		t.Fatalf("search: %v", r)
	}
	a.send(msg{"t": "night_detail", "itemId": 105})
	if dt := a.waitType("night_detail"); dt["item"].(map[string]any)["trailer"] != "yt105" {
		t.Fatalf("detail: %v", dt)
	}

	time.Sleep(1100 * time.Millisecond)
	a.send(msg{"t": "night_pick", "itemId": 100})
	a.send(msg{"t": "night_pick", "itemId": 101})
	a.send(msg{"t": "night_pick", "itemId": 102})
	a.waitType("fun_error") // over the limit of 2
	b.send(msg{"t": "night_pick", "itemId": 101})
	b.send(msg{"t": "night_pick", "itemId": 103})
	a.wait("a picked 2", func(m msg) bool { return m.s("t") == "night" && len(m["mine"].([]any)) == 2 && m.f("pending") == 0 })
	b.wait("b picked 2", func(m msg) bool { return m.s("t") == "night" && len(m["mine"].([]any)) == 2 && m.f("pending") == 0 })

	a.send(msg{"t": "night_done", "ready": true})
	b.send(msg{"t": "night_done", "ready": true})
	deck := b.waitType("deck")
	cards := items(t, deck)
	if len(cards) != 3 {
		t.Fatalf("pool should be deduped to 3: %v", cards)
	}
	if v := waitNight(a, "vote"); v.s("deckId") != deck.s("deckId") {
		t.Fatalf("vote: %v", v)
	}
	for _, c := range []*client{a, b} {
		c.send(msg{"t": "swipe", "deckId": deck.s("deckId"), "itemId": 103, "vote": "like"})
	}
	if m := a.waitType("match"); m["item"].(map[string]any)["id"] != 103.0 {
		t.Fatalf("match: %v", m)
	}
	waitNight(b, "done")

	a.send(msg{"t": "night_close"})
	b.wait("closed", func(m msg) bool { return m.s("t") == "night" && m["closed"] == true })
}
