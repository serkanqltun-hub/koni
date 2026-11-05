package discover

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strconv"
	"strings"
	"testing"
	"time"
)

func TestFilterQuery(t *testing.T) {
	c, err := New("k", "")
	if err != nil {
		t.Fatal(err)
	}
	f := Filter{Genres: []int{53, 27, 10749}, Without: []int{16, 35}, YearFrom: 1995, YearTo: 2024, RuntimeMax: 150, VoteMin: 7, Languages: []string{"en", "KO", "xxx"}}
	q := c.filterQuery(context.Background(), KindTV, f)
	want := map[string]string{
		"with_genres": "9648", "without_genres": "16,35", "first_air_date.gte": "1995-01-01", "first_air_date.lte": "2024-12-31",
		"with_runtime.lte": "150", "with_runtime.gte": "", "vote_average.gte": "7.0", "vote_count.gte": "100", "with_original_language": "en|ko",
	}
	for k, v := range want {
		if got := q.Get(k); got != v {
			t.Errorf("tv %s = %q, want %q", k, got, v)
		}
	}
	q = c.filterQuery(context.Background(), KindMovie, f)
	if q.Get("with_genres") != "53|27|10749" || q.Get("primary_release_date.gte") != "1995-01-01" || q.Get("with_runtime.gte") != "40" {
		t.Errorf("movie query: %v", q)
	}
}

// p0.26 Browse: a page of 30 is cut from TMDB's pages of 20; "new" hides unreleased titles.
func TestBrowsePages(t *testing.T) {
	var last url.Values
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		last = r.URL.Query()
		n, _ := strconv.Atoi(last.Get("page"))
		var res []string
		for i := range 20 {
			res = append(res, fmt.Sprintf(`{"id":%d,"title":"F"}`, (n-1)*20+i))
		}
		fmt.Fprintf(w, `{"total_pages":3,"total_results":60,"results":[%s]}`, strings.Join(res, ","))
	}))
	defer srv.Close()
	c, _ := New("k", "")
	c.TMDBBase = srv.URL
	c.Now = func() time.Time { return time.Date(2026, 11, 5, 0, 0, 0, 0, time.UTC) }
	for page, first := range map[int]int{1: 0, 2: 30} {
		items, total, err := c.Browse(context.Background(), KindMovie, BrowseQuery{Page: page})
		if err != nil || total != 60 || len(items) != 30 || items[0].ID != first || items[29].ID != first+29 {
			t.Fatalf("page %d: %d items from %v, total %d, %v", page, len(items), items[0].ID, total, err)
		}
	}
	c.Browse(context.Background(), KindTV, BrowseQuery{Sort: "new", Lang: "ko"})
	if last.Get("sort_by") != "first_air_date.desc" || last.Get("first_air_date.lte") != "2026-11-05" || last.Get("with_original_language") != "ko" {
		t.Fatalf("new query: %v", last)
	}
}
