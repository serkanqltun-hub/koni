// Package discover is the Santral's "Ne izlesek?" proxy (docs/design/discovery.md §7):
// TMDB discover/search/details and OMDb ratings. API keys stay on the server; every
// upstream response is cached for 24 hours.
package discover

import (
	"context"
	_ "embed"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math/rand/v2"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"sync"
	"time"
)

const (
	CacheTTL = 24 * time.Hour
	DeckSize = 10

	lang       = "tr-TR"
	imgBase    = "https://image.tmdb.org/t/p/w342"
	maxEntries = 4000
	maxPages   = 8 // decks come from the most popular pages only
)

var (
	ErrUnknownMood = errors.New("bilinmeyen mood")
	ErrNotFound    = errors.New("film bulunamadı")
)

//go:embed moods.json
var moodsJSON []byte

// Mood maps a label to TMDB discover filters (§7.2). Lists are OR-ed.
type Mood struct {
	ID           string   `json:"id"`
	Label        string   `json:"label"`
	Emoji        string   `json:"emoji"`
	Genres       []int    `json:"genres,omitempty"`
	Without      []int    `json:"without,omitempty"`
	Keywords     []string `json:"keywords,omitempty"`
	From         string   `json:"from,omitempty"` // primary release date, YYYY-MM-DD
	To           string   `json:"to,omitempty"`
	VoteGte      float64  `json:"voteGte,omitempty"`
	VoteCountGte int      `json:"voteCountGte,omitempty"` // default 300 (quality floor)
	RuntimeLte   int      `json:"runtimeLte,omitempty"`
	Language     string   `json:"language,omitempty"` // original language
}

// MoodInfo is what clients see.
type MoodInfo struct {
	ID    string `json:"id"`
	Label string `json:"label"`
	Emoji string `json:"emoji"`
}

// Ratings: missing sources stay empty (the client greys them out).
type Ratings struct {
	TMDB float64 `json:"tmdb,omitempty"`
	IMDb string  `json:"imdb,omitempty"` // "7.9"
	RT   string  `json:"rt,omitempty"`   // "87%"
	Meta string  `json:"meta,omitempty"` // "74"
}

// Item is one movie card.
type Item struct {
	ID       int      `json:"id"` // TMDB id
	Title    string   `json:"title"`
	Original string   `json:"original,omitempty"`
	Year     int      `json:"year,omitempty"`
	Overview string   `json:"overview,omitempty"`
	Poster   string   `json:"poster,omitempty"`
	Runtime  int      `json:"runtime,omitempty"` // minutes
	Genres   []string `json:"genres,omitempty"`
	IMDbID   string   `json:"imdb,omitempty"`
	Trailer  string   `json:"trailer,omitempty"` // YouTube video id
	Ratings  Ratings  `json:"ratings"`
	// Ne izlesek? v2 (p0.17)
	Kind      string   `json:"kind,omitempty"` // "tv" for series; movies leave it empty
	Seasons   int      `json:"seasons,omitempty"`
	Votes     int      `json:"votes,omitempty"` // TMDB vote count
	Director  string   `json:"director,omitempty"`
	Cast      []string `json:"cast,omitempty"`
	Countries []string `json:"countries,omitempty"`
}

// Client talks to TMDB (required) and OMDb (optional).
type Client struct {
	tmdbKey, omdbKey string
	TMDBBase         string // overridable in tests
	OMDbBase         string
	HTTP             *http.Client
	Now              func() time.Time

	moods []Mood

	mu    sync.Mutex
	cache map[string]entry
}

type entry struct {
	body []byte
	exp  time.Time
}

// New returns a client; tmdbKey is a v3 API key or a v4 read access token.
func New(tmdbKey, omdbKey string) (*Client, error) {
	var moods []Mood
	if err := json.Unmarshal(moodsJSON, &moods); err != nil {
		return nil, fmt.Errorf("moods.json: %w", err)
	}
	return &Client{
		tmdbKey: strings.TrimSpace(tmdbKey), omdbKey: strings.TrimSpace(omdbKey),
		TMDBBase: "https://api.themoviedb.org/3", OMDbBase: "https://www.omdbapi.com/",
		HTTP: &http.Client{Timeout: 8 * time.Second}, Now: time.Now,
		moods: moods, cache: map[string]entry{},
	}, nil
}

func (c *Client) Moods() []MoodInfo {
	out := make([]MoodInfo, len(c.moods))
	for i, m := range c.moods {
		out[i] = MoodInfo{ID: m.ID, Label: m.Label, Emoji: m.Emoji}
	}
	return out
}

// MoodLabel is "🌃 Cyberpunk", or "" for an unknown id.
func (c *Client) MoodLabel(id string) string {
	for _, m := range c.moods {
		if m.ID == id {
			return m.Emoji + " " + m.Label
		}
	}
	return ""
}

// fetch GETs u (keys are added here, so cache keys never contain them) and decodes
// the JSON body into out. Successful bodies are cached for CacheTTL.
func (c *Client) fetch(ctx context.Context, u string, tmdb bool, out any) error {
	now := c.Now()
	c.mu.Lock()
	e, ok := c.cache[u]
	c.mu.Unlock()
	if ok && now.Before(e.exp) {
		return json.Unmarshal(e.body, out)
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, c.withKey(u, tmdb), nil)
	if err != nil {
		return err
	}
	req.Header.Set("Accept", "application/json")
	if tmdb && strings.HasPrefix(c.tmdbKey, "eyJ") {
		req.Header.Set("Authorization", "Bearer "+c.tmdbKey)
	}
	resp, err := c.HTTP.Do(req)
	if err != nil {
		return fmt.Errorf("%s: %w", host(u), err)
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	if err != nil {
		return err
	}
	switch {
	case resp.StatusCode == http.StatusUnauthorized:
		return fmt.Errorf("%s anahtarı geçersiz", host(u))
	case resp.StatusCode != http.StatusOK:
		return fmt.Errorf("%s: HTTP %d", host(u), resp.StatusCode)
	}
	if err := json.Unmarshal(body, out); err != nil {
		return fmt.Errorf("%s: %w", host(u), err)
	}
	c.mu.Lock()
	if len(c.cache) >= maxEntries {
		for k, v := range c.cache {
			if !now.Before(v.exp) {
				delete(c.cache, k)
			}
		}
		if len(c.cache) >= maxEntries {
			c.cache = map[string]entry{}
		}
	}
	c.cache[u] = entry{body: body, exp: now.Add(CacheTTL)}
	c.mu.Unlock()
	return nil
}

func (c *Client) withKey(u string, tmdb bool) string {
	sep := "?"
	if strings.Contains(u, "?") {
		sep = "&"
	}
	switch {
	case tmdb && !strings.HasPrefix(c.tmdbKey, "eyJ"):
		return u + sep + "api_key=" + url.QueryEscape(c.tmdbKey)
	case !tmdb:
		return u + sep + "apikey=" + url.QueryEscape(c.omdbKey)
	}
	return u
}

func host(u string) string {
	if strings.Contains(u, "omdbapi") {
		return "OMDb"
	}
	return "TMDB"
}

func (c *Client) tmdb(ctx context.Context, path string, q url.Values, out any) error {
	return c.fetch(ctx, c.TMDBBase+path+"?"+q.Encode(), true, out)
}

type result struct {
	ID           int     `json:"id"`
	Title        string  `json:"title"`
	Original     string  `json:"original_title"`
	ReleaseDate  string  `json:"release_date"`
	Name         string  `json:"name"` // series
	OriginalName string  `json:"original_name"`
	FirstAirDate string  `json:"first_air_date"`
	Overview     string  `json:"overview"`
	PosterPath   string  `json:"poster_path"`
	VoteAverage  float64 `json:"vote_average"`
	VoteCount    int     `json:"vote_count"`
}

type page struct {
	Results    []result `json:"results"`
	TotalPages int      `json:"total_pages"`
}

func (r result) item() Item { return r.itemOf(KindMovie) }

func (r result) itemOf(kind string) Item {
	if kind == KindTV {
		r.Title, r.Original, r.ReleaseDate = r.Name, r.OriginalName, r.FirstAirDate
	}
	it := Item{ID: r.ID, Title: r.Title, Overview: r.Overview, Votes: r.VoteCount, Ratings: Ratings{TMDB: r.VoteAverage}}
	if kind == KindTV {
		it.Kind = KindTV
	}
	if r.Original != r.Title {
		it.Original = r.Original
	}
	if len(r.ReleaseDate) >= 4 {
		it.Year, _ = strconv.Atoi(r.ReleaseDate[:4])
	}
	if r.PosterPath != "" {
		it.Poster = imgBase + r.PosterPath
	}
	return it
}

func joinInts(xs []int) string {
	s := make([]string, len(xs))
	for i, x := range xs {
		s[i] = strconv.Itoa(x)
	}
	return strings.Join(s, "|")
}

// keywordIDs resolves names with /search/keyword (cached); unknown names are skipped.
func (c *Client) keywordIDs(ctx context.Context, names []string) []int {
	var ids []int
	for _, n := range names {
		var p struct {
			Results []struct {
				ID   int    `json:"id"`
				Name string `json:"name"`
			} `json:"results"`
		}
		if err := c.tmdb(ctx, "/search/keyword", url.Values{"query": {n}}, &p); err != nil || len(p.Results) == 0 {
			continue
		}
		id := p.Results[0].ID
		for _, r := range p.Results {
			if strings.EqualFold(r.Name, n) {
				id = r.ID
				break
			}
		}
		ids = append(ids, id)
	}
	return ids
}

// Deck returns up to DeckSize popular movies for the mood that are not in seen,
// shuffled, with details and ratings. maxRuntime (minutes, 0 = any) tightens the mood.
func (c *Client) Deck(ctx context.Context, moodID string, maxRuntime int, seen map[int]bool) ([]Item, error) {
	var m *Mood
	for i := range c.moods {
		if c.moods[i].ID == moodID {
			m = &c.moods[i]
		}
	}
	if m == nil {
		return nil, ErrUnknownMood
	}
	q := url.Values{"language": {lang}, "include_adult": {"false"}, "sort_by": {"popularity.desc"}}
	vc := 300
	if m.VoteCountGte > 0 {
		vc = m.VoteCountGte
	}
	q.Set("vote_count.gte", strconv.Itoa(vc))
	if len(m.Genres) > 0 {
		q.Set("with_genres", joinInts(m.Genres))
	}
	if len(m.Without) > 0 {
		q.Set("without_genres", joinInts(m.Without))
	}
	if len(m.Keywords) > 0 {
		ids := c.keywordIDs(ctx, m.Keywords)
		if len(ids) > 0 {
			q.Set("with_keywords", joinInts(ids))
		}
	}
	if m.From != "" {
		q.Set("primary_release_date.gte", m.From)
	}
	if m.To != "" {
		q.Set("primary_release_date.lte", m.To)
	}
	if m.VoteGte > 0 {
		q.Set("vote_average.gte", strconv.FormatFloat(m.VoteGte, 'f', -1, 64))
	}
	rt := m.RuntimeLte
	if maxRuntime > 0 && (rt == 0 || maxRuntime < rt) {
		rt = maxRuntime
	}
	if rt > 0 {
		q.Set("with_runtime.lte", strconv.Itoa(rt))
		q.Set("with_runtime.gte", "40") // no shorts
	}
	if m.Language != "" {
		q.Set("with_original_language", m.Language)
	}

	q.Set("page", "1")
	var first page
	if err := c.tmdb(ctx, "/discover/movie", q, &first); err != nil {
		return nil, err
	}
	pages := min(max(first.TotalPages, 1), maxPages)
	var pool []result
	add := func(p page) {
		for _, r := range p.Results {
			if !seen[r.ID] {
				pool = append(pool, r)
			}
		}
	}
	// a random popular page first, then the following ones until the deck is full
	start := rand.IntN(pages) + 1
	for i := 0; i < pages && len(pool) < DeckSize*2; i++ {
		n := (start+i-1)%pages + 1
		if n == 1 {
			add(first)
			continue
		}
		q.Set("page", strconv.Itoa(n))
		var p page
		if err := c.tmdb(ctx, "/discover/movie", q, &p); err != nil {
			return nil, err
		}
		add(p)
	}
	if len(pool) == 0 {
		return nil, ErrNotFound
	}
	rand.Shuffle(len(pool), func(i, j int) { pool[i], pool[j] = pool[j], pool[i] })
	pool = pool[:min(len(pool), DeckSize)]
	items := make([]Item, len(pool))
	var wg sync.WaitGroup
	for i, r := range pool {
		wg.Add(1)
		go func() {
			defer wg.Done()
			items[i] = c.details(ctx, r.item())
		}()
	}
	wg.Wait()
	return items, nil
}

// Search returns the best match for a typed title (wheel proposals).
func (c *Client) Search(ctx context.Context, query string) (Item, error) {
	var p page
	q := url.Values{"query": {query}, "language": {lang}, "include_adult": {"false"}}
	if err := c.tmdb(ctx, "/search/movie", q, &p); err != nil {
		return Item{}, err
	}
	if len(p.Results) == 0 {
		return Item{}, ErrNotFound
	}
	return c.details(ctx, p.Results[0].item()), nil
}

// details adds runtime, genres, IMDb id, cast, a trailer and OMDb ratings; failures
// keep the card as it is (missing parts are shown grey). it.Kind picks movie or series;
// a card with only an id gets its title, poster and year here too.
func (c *Client) details(ctx context.Context, it Item) Item {
	var d struct {
		result
		Runtime     int    `json:"runtime"`
		EpisodeRun  []int  `json:"episode_run_time"`
		Seasons     int    `json:"number_of_seasons"`
		IMDbID      string `json:"imdb_id"`
		ExternalIDs struct {
			IMDbID string `json:"imdb_id"`
		} `json:"external_ids"`
		Genres []struct {
			Name string `json:"name"`
		} `json:"genres"`
		Countries []struct {
			Name string `json:"name"`
		} `json:"production_countries"`
		CreatedBy []struct {
			Name string `json:"name"`
		} `json:"created_by"`
		Credits struct {
			Cast []struct {
				Name string `json:"name"`
			} `json:"cast"`
			Crew []struct {
				Name string `json:"name"`
				Job  string `json:"job"`
			} `json:"crew"`
		} `json:"credits"`
		Videos struct {
			Results []struct {
				Key  string `json:"key"`
				Site string `json:"site"`
				Type string `json:"type"`
				Lang string `json:"iso_639_1"`
			} `json:"results"`
		} `json:"videos"`
	}
	kind, extra := KindMovie, "videos,credits"
	if it.Kind == KindTV {
		kind, extra = KindTV, "videos,credits,external_ids"
	}
	q := url.Values{"language": {lang}, "append_to_response": {extra}, "include_video_language": {"tr,en,null"}}
	if err := c.tmdb(ctx, "/"+kind+"/"+strconv.Itoa(it.ID), q, &d); err == nil {
		if it.Title == "" {
			d.ID = it.ID
			base := d.result.itemOf(kind)
			base.Ratings = it.Ratings
			if base.Ratings.TMDB == 0 {
				base.Ratings.TMDB = d.VoteAverage
			}
			it = base
		}
		it.Runtime, it.IMDbID = d.Runtime, d.IMDbID
		if kind == KindTV {
			it.IMDbID, it.Seasons = d.ExternalIDs.IMDbID, d.Seasons
			if len(d.EpisodeRun) > 0 {
				it.Runtime = d.EpisodeRun[0]
			}
		}
		if it.Overview == "" {
			it.Overview = d.Overview
		}
		if it.Votes == 0 {
			it.Votes = d.VoteCount
		}
		it.Genres = nil
		for _, g := range d.Genres {
			it.Genres = append(it.Genres, g.Name)
		}
		it.Countries = nil
		for _, x := range d.Countries {
			if len(it.Countries) < 2 {
				it.Countries = append(it.Countries, x.Name)
			}
		}
		for _, p := range d.Credits.Crew {
			if p.Job == "Director" {
				it.Director = p.Name
				break
			}
		}
		if it.Director == "" && len(d.CreatedBy) > 0 {
			it.Director = d.CreatedBy[0].Name
		}
		it.Cast = nil
		for _, p := range d.Credits.Cast {
			if len(it.Cast) == 4 {
				break
			}
			it.Cast = append(it.Cast, p.Name)
		}
		best := -1
		for _, v := range d.Videos.Results {
			if v.Site != "YouTube" {
				continue
			}
			score := 0
			switch v.Type {
			case "Trailer":
				score = 4
			case "Teaser":
				score = 2
			default:
				continue
			}
			if v.Lang == "tr" {
				score++
			}
			if score > best {
				best, it.Trailer = score, v.Key
			}
		}
	}
	if c.omdbKey != "" && it.IMDbID != "" {
		var o struct {
			IMDbRating string `json:"imdbRating"`
			Metascore  string `json:"Metascore"`
			Ratings    []struct {
				Source string `json:"Source"`
				Value  string `json:"Value"`
			} `json:"Ratings"`
		}
		if err := c.fetch(ctx, c.OMDbBase+"?i="+url.QueryEscape(it.IMDbID), false, &o); err == nil {
			if o.IMDbRating != "N/A" {
				it.Ratings.IMDb = o.IMDbRating
			}
			if o.Metascore != "N/A" {
				it.Ratings.Meta = o.Metascore
			}
			for _, r := range o.Ratings {
				if r.Source == "Rotten Tomatoes" {
					it.Ratings.RT = r.Value
				}
			}
		}
	}
	return it
}
