package hub

// Monthly recap (p0.28, PROTOCOL.md §3.0q): one-shot `recap`.

import (
	"cmp"
	"context"
	"slices"
	"strings"
	"sync"

	"github.com/serkanqltun-hub/koni/server/store"
)

// recapGenreTitles: how many of the month's titles are looked up for genres.
const recapGenreTitles = 12

type genreCount struct {
	Name  string `json:"name"`
	Count int    `json:"count"`
}

type recapBadge struct {
	ID    string `json:"id"`
	Title string `json:"title"`
	Rule  string `json:"rule"`
}

// Recap answers `recap`: the account's month in numbers.
func (h *Hub) Recap(ctx context.Context, in Inbound) map[string]any {
	u, e := h.account(in.Auth)
	if e != nil {
		return e
	}
	from, to, month, ok := store.MonthRange(in.Month, h.cfg.Now())
	if !ok {
		return errMsg("bad_period")
	}
	st := h.cfg.Store
	data, err := st.Recap(u.ID, from, to)
	if err != nil {
		return errMsg(errCode(err))
	}
	stats, err := st.StatsRange(u.ID, from, to)
	if err != nil {
		return errMsg(errCode(err))
	}
	badges := []recapBadge{}
	for _, b := range store.Badges(stats) {
		if slices.Contains(b.Users, u.Handle()) {
			badges = append(badges, recapBadge{b.ID, b.Title, b.Rule})
		}
	}
	return map[string]any{
		"t": "recap", "month": month, "watchSec": data.WatchSec, "days": data.Days,
		"hours": data.Hours, "peakHour": data.PeakHour, "movies": data.Movies, "series": data.Series,
		"genres": h.recapGenres(ctx, data.Seen), "mates": data.Mates, "badges": badges, "best": data.Best,
	}
}

// recapGenres counts the genres of the month's titles (TMDB), most first, top 5.
func (h *Hub) recapGenres(ctx context.Context, seen []store.Entry) []genreCount {
	out := []genreCount{}
	d := h.cfg.Discover
	if d == nil || len(seen) == 0 {
		return out
	}
	seen = slices.DeleteFunc(slices.Clone(seen), func(e store.Entry) bool {
		return !strings.HasPrefix(e.Ref, "tt") && !strings.HasPrefix(e.Ref, "tmdb:")
	})
	if len(seen) > recapGenreTitles {
		seen = seen[:recapGenreTitles]
	}
	ctx, cancel := context.WithTimeout(ctx, upstreamTimeout)
	defer cancel()
	var mu sync.Mutex
	var wg sync.WaitGroup
	counts := map[string]int{}
	for _, x := range seen {
		wg.Add(1)
		go func() {
			defer wg.Done()
			kind, id, err := resolveRef(ctx, d, x.Ref, x.Kind)
			if err != nil {
				return
			}
			it, err := d.Detail(ctx, kind, id)
			if err != nil {
				return
			}
			mu.Lock()
			for _, g := range it.Genres {
				counts[g]++
			}
			mu.Unlock()
		}()
	}
	wg.Wait()
	for g, n := range counts {
		out = append(out, genreCount{g, n})
	}
	slices.SortFunc(out, func(a, b genreCount) int {
		if c := cmp.Compare(b.Count, a.Count); c != 0 {
			return c
		}
		return strings.Compare(a.Name, b.Name)
	})
	if len(out) > 5 {
		out = out[:5]
	}
	return out
}
