package discover

import (
	"context"
	"math/rand/v2"
	"net/url"
	"slices"
	"strconv"
	"strings"
	"sync"
)

// "Ne izlesek?" v2 (docs/PROTOCOL.md §3.9a): every member builds their own filter,
// gets suggestions for it and searches freely. Films and series both work.

const (
	KindMovie = "movie"
	KindTV    = "tv"

	SuggestSize = 20
	searchSize  = 8
)

// Filter is one member's filter. Genre ids are TMDB movie genres; for series they
// map to the closest TV genre (or are dropped).
type Filter struct {
	Genres       []int    `json:"genres,omitempty"`  // any of
	Without      []int    `json:"without,omitempty"` // none of
	Keywords     []string `json:"keywords,omitempty"`
	YearFrom     int      `json:"yearFrom,omitempty"`
	YearTo       int      `json:"yearTo,omitempty"`
	RuntimeMin   int      `json:"runtimeMin,omitempty"` // minutes (series: episode)
	RuntimeMax   int      `json:"runtimeMax,omitempty"`
	VoteMin      float64  `json:"voteMin,omitempty"`      // TMDB 0–10
	VoteCountMin int      `json:"voteCountMin,omitempty"` // default 300 (series 100)
	Languages    []string `json:"languages,omitempty"`    // original language, any of
	HideSeen     bool     `json:"hideSeen,omitempty"`     // skip what the room was dealt before
}

// Tagged is a filter with the client's name for it (previews).
type Tagged struct {
	Tag    string `json:"tag"`
	Filter Filter `json:"filter"`
}

// Preview: how many titles a filter matches and a few posters for its tile.
type Preview struct {
	Tag     string   `json:"tag"`
	Total   int      `json:"total"`
	Posters []string `json:"posters"`
}

func ValidKind(k string) bool { return k == KindMovie || k == KindTV }

var tvGenre = map[int]int{
	28: 10759, 12: 10759, // aksiyon, macera
	14: 10765, 878: 10765, // fantastik, bilimkurgu
	10752: 10768,          // savaş
	53:    9648, 27: 9648, // gerilim, korku → gizem
	16: 16, 35: 35, 80: 80, 99: 99, 18: 18, 10751: 10751, 9648: 9648, 37: 37,
}

func genresFor(kind string, ids []int) []int {
	var out []int
	for _, id := range ids {
		if kind == KindTV {
			id = tvGenre[id]
		}
		if id != 0 && !slices.Contains(out, id) {
			out = append(out, id)
		}
	}
	return out
}

func (c *Client) filterQuery(ctx context.Context, kind string, f Filter) url.Values {
	q := url.Values{"language": {lang}, "include_adult": {"false"}, "sort_by": {"popularity.desc"}}
	vc := 300
	if kind == KindTV {
		vc = 100
	}
	if f.VoteCountMin > 0 {
		vc = min(f.VoteCountMin, 100000)
	}
	q.Set("vote_count.gte", strconv.Itoa(vc))
	if g := genresFor(kind, f.Genres); len(g) > 0 {
		q.Set("with_genres", joinInts(g))
	}
	if g := genresFor(kind, f.Without); len(g) > 0 {
		q.Set("without_genres", strings.ReplaceAll(joinInts(g), "|", ","))
	}
	if len(f.Keywords) > 0 {
		if ids := c.keywordIDs(ctx, f.Keywords[:min(len(f.Keywords), 3)]); len(ids) > 0 {
			q.Set("with_keywords", joinInts(ids))
		}
	}
	date := "primary_release_date"
	if kind == KindTV {
		date = "first_air_date"
	}
	if f.YearFrom > 0 {
		q.Set(date+".gte", strconv.Itoa(f.YearFrom)+"-01-01")
	}
	if f.YearTo > 0 {
		q.Set(date+".lte", strconv.Itoa(f.YearTo)+"-12-31")
	}
	if f.VoteMin > 0 {
		q.Set("vote_average.gte", strconv.FormatFloat(min(f.VoteMin, 10), 'f', 1, 64))
	}
	if f.RuntimeMin > 0 {
		q.Set("with_runtime.gte", strconv.Itoa(f.RuntimeMin))
	} else if f.RuntimeMax > 0 && kind == KindMovie {
		q.Set("with_runtime.gte", "40") // no shorts
	}
	if f.RuntimeMax > 0 {
		q.Set("with_runtime.lte", strconv.Itoa(f.RuntimeMax))
	}
	var langs []string
	for _, l := range f.Languages {
		if len(l) == 2 {
			langs = append(langs, strings.ToLower(l))
		}
	}
	if len(langs) > 0 {
		q.Set("with_original_language", strings.Join(langs, "|"))
	}
	return q
}

type countPage struct {
	page
	TotalResults int `json:"total_results"`
}

// Suggest returns up to SuggestSize popular titles for the filter (not in skip),
// shuffled, without details: the grid needs only poster, title, year and score.
func (c *Client) Suggest(ctx context.Context, kind string, f Filter, skip map[int]bool) ([]Item, int, error) {
	q := c.filterQuery(ctx, kind, f)
	q.Set("page", "1")
	var first countPage
	if err := c.tmdb(ctx, "/discover/"+kind, q, &first); err != nil {
		return nil, 0, err
	}
	pages := min(max(first.TotalPages, 1), maxPages)
	var pool []result
	add := func(p page) {
		for _, r := range p.Results {
			if !skip[r.ID] {
				pool = append(pool, r)
			}
		}
	}
	start := rand.IntN(pages) + 1
	for i := 0; i < pages && len(pool) < SuggestSize; i++ {
		n := (start+i-1)%pages + 1
		if n == 1 {
			add(first.page)
			continue
		}
		q.Set("page", strconv.Itoa(n))
		var p page
		if err := c.tmdb(ctx, "/discover/"+kind, q, &p); err != nil {
			return nil, 0, err
		}
		add(p)
	}
	rand.Shuffle(len(pool), func(i, j int) { pool[i], pool[j] = pool[j], pool[i] })
	pool = pool[:min(len(pool), SuggestSize)]
	out := make([]Item, len(pool))
	for i, r := range pool {
		out[i] = r.itemOf(kind)
	}
	return out, first.TotalResults, nil
}

// Previews counts each filter's matches (one cached request each).
func (c *Client) Previews(ctx context.Context, kind string, fs []Tagged) []Preview {
	out := make([]Preview, len(fs))
	var wg sync.WaitGroup
	for i, t := range fs {
		wg.Add(1)
		go func() {
			defer wg.Done()
			out[i] = Preview{Tag: t.Tag, Posters: []string{}}
			q := c.filterQuery(ctx, kind, t.Filter)
			q.Set("page", "1")
			var p countPage
			if c.tmdb(ctx, "/discover/"+kind, q, &p) != nil {
				return
			}
			out[i].Total = p.TotalResults
			for _, r := range p.Results {
				if r.PosterPath != "" && len(out[i].Posters) < 3 {
					out[i].Posters = append(out[i].Posters, imgBase+r.PosterPath)
				}
			}
		}()
	}
	wg.Wait()
	return out
}

// SearchList: typed titles, best matches first, without details.
func (c *Client) SearchList(ctx context.Context, kind, query string) ([]Item, error) {
	var p page
	q := url.Values{"query": {query}, "language": {lang}, "include_adult": {"false"}}
	if err := c.tmdb(ctx, "/search/"+kind, q, &p); err != nil {
		return nil, err
	}
	out := []Item{}
	for _, r := range p.Results {
		if len(out) == searchSize {
			break
		}
		out = append(out, r.itemOf(kind))
	}
	return out, nil
}

// Detail is the full card (künye): runtime, genres, cast, trailer, ratings.
// FindIMDb resolves an IMDb id ("tt…") to a TMDB id of kind (p0.23: picks from
// profile lists, which are keyed by IMDb id).
func (c *Client) FindIMDb(ctx context.Context, kind, imdb string) (int, error) {
	var r struct {
		Movies []struct{ ID int } `json:"movie_results"`
		TV     []struct{ ID int } `json:"tv_results"`
	}
	if err := c.tmdb(ctx, "/find/"+url.PathEscape(imdb), url.Values{"external_source": {"imdb_id"}}, &r); err != nil {
		return 0, err
	}
	if kind == KindTV && len(r.TV) > 0 {
		return r.TV[0].ID, nil
	}
	if kind != KindTV && len(r.Movies) > 0 {
		return r.Movies[0].ID, nil
	}
	return 0, ErrNotFound
}

func (c *Client) Detail(ctx context.Context, kind string, id int) (Item, error) {
	it := c.details(ctx, Item{ID: id, Kind: kind})
	if it.Title == "" {
		return it, ErrNotFound
	}
	return it, nil
}
