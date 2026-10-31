package discover

import (
	"context"
	"testing"
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
