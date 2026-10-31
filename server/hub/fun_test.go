package hub_test

import (
	"encoding/json"
	"fmt"
	"math"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/serkanqltun-hub/koni/server/discover"
	"github.com/serkanqltun-hub/koni/server/hub"
)

// fakeTMDB serves 12 popular movies (ids 100–111), details and a title search.
func fakeTMDB(t *testing.T) *httptest.Server {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		switch p := r.URL.Path; {
		case p == "/search/keyword":
			fmt.Fprintf(w, `{"results":[{"id":7,"name":%q}]}`, r.URL.Query().Get("query"))
		case p == "/discover/movie":
			var res []string
			for id := 100; id < 112; id++ {
				res = append(res, fmt.Sprintf(`{"id":%d,"title":"Film %d","release_date":"1999-01-01","vote_average":7.5,"poster_path":"/p%d.jpg"}`, id, id, id))
			}
			fmt.Fprintf(w, `{"total_pages":1,"results":[%s]}`, strings.Join(res, ","))
		case p == "/search/movie":
			fmt.Fprint(w, `{"results":[{"id":500,"title":"Matrix","release_date":"1999-03-31"}]}`)
		case strings.HasPrefix(p, "/movie/"):
			id := strings.TrimPrefix(p, "/movie/")
			fmt.Fprintf(w, `{"title":"Film %s","runtime":120,"imdb_id":"tt%s","genres":[{"name":"Dram"}],"videos":{"results":[{"key":"teaser%s","site":"YouTube","type":"Teaser"},{"key":"yt%s","site":"YouTube","type":"Trailer","iso_639_1":"en"}]}}`, id, id, id, id)
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(srv.Close)
	return srv
}

func items(t *testing.T, m msg) []map[string]any {
	t.Helper()
	raw, _ := json.Marshal(m["items"])
	var out []map[string]any
	if err := json.Unmarshal(raw, &out); err != nil {
		t.Fatal(err)
	}
	return out
}

func TestDiscoverSwipeAndWheel(t *testing.T) {
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
	if moods := a.waitType("moods"); len(moods["moods"].([]any)) < 10 {
		t.Fatalf("moods: %v", moods)
	}
	b := dial(t, srv)
	b.send(msg{"t": "join", "invite": created.s("invite"), "username": "Berk"})
	b.waitType("moods")

	a.send(msg{"t": "discover", "mood": "comfort"})
	deck := b.waitType("deck")
	a.waitType("deck")
	cards := items(t, deck)
	if len(cards) != discover.DeckSize || cards[0]["trailer"] != fmt.Sprintf("yt%.0f", cards[0]["id"]) || cards[0]["runtime"] != 120.0 {
		t.Fatalf("deck: %v", cards[0])
	}
	// the first card both like is the match; earlier cards one of them passed
	id := deck.s("deckId")
	pick, other := cards[3]["id"], cards[1]["id"]
	a.send(msg{"t": "swipe", "deckId": id, "itemId": other, "vote": "like"})
	b.send(msg{"t": "swipe", "deckId": id, "itemId": other, "vote": "pass"})
	a.send(msg{"t": "swipe", "deckId": id, "itemId": pick, "vote": "like"})
	b.send(msg{"t": "swipe", "deckId": id, "itemId": pick, "vote": "like"})
	for _, c := range []*client{a, b} {
		if m := c.waitType("match"); m["item"].(map[string]any)["id"] != pick {
			t.Fatalf("match: %v", m)
		}
	}

	// next deck: only the two movies not dealt yet; nobody likes them → wheel
	a.send(msg{"t": "discover", "mood": "comfort"})
	deck2 := a.waitType("deck")
	cards2 := items(t, deck2)
	for _, c := range cards2 {
		for _, old := range cards {
			if c["id"] == old["id"] {
				t.Fatalf("movie %v dealt twice", c["id"])
			}
		}
	}
	if len(cards2) != 2 {
		t.Fatalf("second deck has %d cards", len(cards2))
	}
	for _, c := range []*client{a, b} {
		for _, card := range cards2 {
			c.send(msg{"t": "swipe", "deckId": deck2.s("deckId"), "itemId": card["id"], "vote": "pass"})
		}
	}
	wheel := a.waitType("wheel")
	if ps := wheel["proposals"].([]any); len(ps) != 2 {
		t.Fatalf("deck → wheel: %v", wheel)
	}

	// a typed proposal is resolved to a card; then spin
	b.send(msg{"t": "propose", "query": "matrix"})
	for {
		w := a.waitType("wheel")
		if ps := w["proposals"].([]any); len(ps) == 3 {
			if it := ps[2].(map[string]any)["item"].(map[string]any); it["id"] != 500.0 || it["title"] != "Matrix" {
				t.Fatalf("typed proposal: %v", it)
			}
			break
		}
	}
	a.send(msg{"t": "spin"})
	for {
		w := b.waitType("wheel")
		res, ok := w["result"].(map[string]any)
		if !ok {
			continue
		}
		n := float64(len(w["proposals"].([]any)))
		turns, idx := res["turns"].(float64), res["index"].(float64)
		// slice under the top pointer after turning clockwise by `turns`
		frac := turns - math.Floor(turns)
		if got := math.Floor((1 - frac) * n); got != idx || turns < 5 {
			t.Fatalf("pointer lands on slice %v, winner is %v (turns %v)", got, idx, turns)
		}
		if w["proposals"].([]any)[int(idx)].(map[string]any)["id"] != res["winner"] {
			t.Fatalf("winner id mismatch: %v", w)
		}
		break
	}
}
