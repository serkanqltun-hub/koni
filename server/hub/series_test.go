package hub_test

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/serkanqltun-hub/koni/server/discover"
	"github.com/serkanqltun-hub/koni/server/hub"
	"github.com/serkanqltun-hub/koni/server/store"
)

// seriesTMDB: two series (tt900 airs next week, tt901 ended after 25 episodes) and
// movies whose similar titles are 101–103.
func seriesTMDB(t *testing.T) *httptest.Server {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch p := r.URL.Path; {
		case strings.HasPrefix(p, "/find/tt9"):
			fmt.Fprintf(w, `{"movie_results":[],"tv_results":[{"id":%s}]}`, strings.TrimPrefix(p, "/find/tt"))
		case strings.HasPrefix(p, "/find/"):
			fmt.Fprintf(w, `{"movie_results":[{"id":%s}],"tv_results":[]}`, strings.TrimPrefix(p, "/find/tt"))
		case p == "/tv/900":
			fmt.Fprint(w, `{"name":"Yeni Dizi","poster_path":"/d.jpg","last_episode_to_air":{"season_number":1,"episode_number":8,"air_date":"2026-06-01"},
"next_episode_to_air":{"season_number":2,"episode_number":1,"air_date":"2026-11-10","name":"Dönüş"},
"seasons":[{"season_number":0,"episode_count":3},{"season_number":1,"episode_count":8},{"season_number":2,"episode_count":8}]}`)
		case p == "/tv/901":
			fmt.Fprint(w, `{"name":"Bitti","last_episode_to_air":{"season_number":3,"episode_number":5},
"seasons":[{"season_number":1,"episode_count":10},{"season_number":2,"episode_count":10},{"season_number":3,"episode_count":5}]}`)
		case strings.HasPrefix(p, "/movie/") && strings.HasSuffix(p, "/similar"):
			fmt.Fprint(w, `{"results":[{"id":101,"title":"Film 101"},{"id":102,"title":"Film 102"},{"id":103,"title":"Film 103"}]}`)
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(srv.Close)
	return srv
}

// seriesServer: an account server on seriesTMDB with mutual friends Ayse and Berk.
func seriesServer(t *testing.T) (srv *httptest.Server, st *store.Store, ta, tb string) {
	t.Helper()
	d, err := discover.New("test-key", "")
	if err != nil {
		t.Fatal(err)
	}
	d.TMDBBase = seriesTMDB(t).URL
	st, err = store.Open(filepath.Join(t.TempDir(), "koni.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { st.Close() })
	srv = httptest.NewServer(hub.New(hub.Config{MinLead: 30 * time.Millisecond, Store: st, Discover: d}).Handler())
	t.Cleanup(srv.Close)
	code, _ := st.CreateInvite(0, time.Hour)
	ta = handshake(t, srv, msg{"t": "register", "code": code, "username": "Ayse", "password": "uzun-sifre"}).s("token")
	member := handshake(t, srv, msg{"t": "profile", "auth": ta, "op": "invite"}).s("code")
	tb = handshake(t, srv, msg{"t": "register", "code": member, "username": "Berk", "password": "uzun-sifre"}).s("token")
	if ta == "" || tb == "" {
		t.Fatal("register failed")
	}
	return srv, st, ta, tb
}

func ok(t *testing.T, r msg) msg {
	t.Helper()
	if r.s("t") == "error" {
		t.Fatalf("error: %v", r)
	}
	return r
}

func decode[T any](t *testing.T, v any) T {
	t.Helper()
	raw, _ := json.Marshal(v)
	var out T
	if err := json.Unmarshal(raw, &out); err != nil {
		t.Fatal(err)
	}
	return out
}

// p0.27 `upcoming`: series on want/progress with their next episode and unseen
// count, plus the team's last episode and who is ahead.
func TestUpcoming(t *testing.T) {
	srv, st, ta, tb := seriesServer(t)
	if r := handshake(t, srv, msg{"t": "upcoming"}); r.s("code") != "auth_required" {
		t.Fatalf("no auth: %v", r)
	}
	ok(t, handshake(t, srv, msg{"t": "lists", "auth": ta, "op": "add", "list": "want", "entry": msg{"ref": "tt900", "kind": "series", "title": "Yeni"}}))
	ok(t, handshake(t, srv, msg{"t": "lists", "auth": ta, "op": "add", "list": "want", "entry": msg{"ref": "tt100", "kind": "movie", "title": "Film"}}))
	ok(t, handshake(t, srv, msg{"t": "lists", "auth": ta, "op": "progress", "ref": "tt901", "kind": "series", "season": 1, "episode": 2, "pos": 10, "title": "Bitti"}))
	for _, e := range []int{1, 2} {
		ok(t, handshake(t, srv, msg{"t": "lists", "auth": ta, "op": "seen_ep", "ref": "tt901", "season": 1, "episode": e}))
	}

	a, _ := st.UserByToken(ta)
	b, _ := st.UserByToken(tb)
	if err := st.SetTeamProgress([]int64{a.ID, b.ID}, "tt901", "Bitti", "Cuma", 2, 4); err != nil {
		t.Fatal(err)
	}
	if err := st.SetTeamProgress([]int64{b.ID, a.ID}, "tt901", "Bitti", "Cuma", 1, 9); err != nil {
		t.Fatal(err) // an earlier episode doesn't move the team back
	}
	ok(t, handshake(t, srv, msg{"t": "lists", "auth": tb, "op": "seen_ep", "ref": "tt901", "season": 2, "episode": 5}))

	r := ok(t, handshake(t, srv, msg{"t": "upcoming", "auth": ta}))
	type item struct {
		Ref, Title, Name, Airs string
		Season, Episode        int
		Unseen                 int
	}
	its := decode[[]item](t, r["items"])
	want := []item{
		{Ref: "tt900", Title: "Yeni", Name: "Dönüş", Airs: "2026-11-10", Season: 2, Episode: 1, Unseen: 8},
		{Ref: "tt901", Title: "Bitti", Unseen: 23},
	}
	if fmt.Sprint(its) != fmt.Sprint(want) {
		t.Fatalf("items: %+v", its)
	}
	team := decode[[]store.Team](t, r["team"])
	if len(team) != 1 || team[0].Season != 2 || team[0].Episode != 4 || team[0].Room != "Cuma" ||
		len(team[0].Members) != 1 || !strings.HasPrefix(team[0].Members[0].Name, "Berk#") || len(team[0].Ahead) != 1 {
		t.Fatalf("team: %+v", team)
	}
	// Berk sees the same team without himself and nobody ahead.
	team = decode[[]store.Team](t, ok(t, handshake(t, srv, msg{"t": "upcoming", "auth": tb}))["team"])
	if len(team) != 1 || !strings.HasPrefix(team[0].Members[0].Name, "Ayse#") || len(team[0].Ahead) != 0 {
		t.Fatalf("berk team: %+v", team)
	}
}

// p0.27 `skip`: marks per season, shared and last writer wins.
func TestSkip(t *testing.T) {
	srv, _, ta, tb := seriesServer(t)
	if r := handshake(t, srv, msg{"t": "skip", "op": "get", "ref": "tt900"}); r.s("code") != "auth_required" {
		t.Fatalf("no auth: %v", r)
	}
	if r := handshake(t, srv, msg{"t": "skip", "auth": ta, "op": "set", "ref": "tt900", "introStart": 90, "introEnd": 60}); r.s("code") != "bad_entry" {
		t.Fatalf("bad mark: %v", r)
	}
	ok(t, handshake(t, srv, msg{"t": "skip", "auth": ta, "op": "set", "ref": "tt900", "introStart": 30, "introEnd": 95}))
	ok(t, handshake(t, srv, msg{"t": "skip", "auth": ta, "op": "set", "ref": "tt900", "season": 2, "introStart": 10, "introEnd": 70, "recapEnd": 8}))
	ok(t, handshake(t, srv, msg{"t": "skip", "auth": tb, "op": "set", "ref": "tt900", "introStart": 31, "introEnd": 96}))
	marks := decode[[]store.Skip](t, ok(t, handshake(t, srv, msg{"t": "skip", "auth": ta, "op": "get", "ref": "tt900"}))["marks"])
	if len(marks) != 2 || marks[0].Season != 0 || marks[0].IntroEnd != 96 || !strings.HasPrefix(marks[0].By, "Berk#") ||
		marks[1].Season != 2 || marks[1].RecapEnd != 8 {
		t.Fatalf("marks: %+v", marks)
	}
	marks = decode[[]store.Skip](t, ok(t, handshake(t, srv, msg{"t": "skip", "auth": ta, "op": "clear", "ref": "tt900", "season": 2}))["marks"])
	if len(marks) != 1 || marks[0].Season != 0 {
		t.Fatalf("after clear: %+v", marks)
	}
}

// p0.27 `recs`: similar to a 4+ star title (minus your lists), friends' 4+ stars
// you haven't seen, and want titles a friend wants too.
func TestRecs(t *testing.T) {
	srv, _, ta, tb := seriesServer(t)
	addList(t, srv, ta, "seen", "tt100")
	ok(t, handshake(t, srv, msg{"t": "lists", "auth": ta, "op": "rate", "ref": "tt100", "stars": 5}))
	addList(t, srv, ta, "want", "tmdb:movie:102")
	addList(t, srv, tb, "seen", "tt555")
	ok(t, handshake(t, srv, msg{"t": "lists", "auth": tb, "op": "rate", "ref": "tt555", "stars": 4, "word": "harika"}))
	addList(t, srv, tb, "seen", "tt100") // Ayşe saw it: not a friends' rec
	ok(t, handshake(t, srv, msg{"t": "lists", "auth": tb, "op": "rate", "ref": "tt100", "stars": 5}))
	addList(t, srv, ta, "want", "tt777")
	addList(t, srv, tb, "want", "tt777")

	r := ok(t, handshake(t, srv, msg{"t": "recs", "auth": ta}))
	because := decode[[]struct {
		Ref   string
		Items []struct{ ID int }
	}](t, r["because"])
	if len(because) != 1 || because[0].Ref != "tt100" || fmt.Sprint(because[0].Items) != "[{101} {103}]" {
		t.Fatalf("because: %+v", because)
	}
	friends := decode[[]store.Liked](t, r["friends"])
	if len(friends) != 1 || friends[0].Ref != "tt555" || len(friends[0].By) != 1 || friends[0].By[0].Word != "harika" {
		t.Fatalf("friends: %+v", friends)
	}
	together := decode[[]struct {
		Ref   string
		Mates []store.Holder
	}](t, r["together"])
	if len(together) != 1 || together[0].Ref != "tt777" || !strings.HasPrefix(together[0].Mates[0].Name, "Berk#") {
		t.Fatalf("together: %+v", together)
	}
}
