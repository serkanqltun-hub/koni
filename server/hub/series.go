package hub

import (
	"context"
	"slices"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/serkanqltun-hub/koni/server/discover"
	"github.com/serkanqltun-hub/koni/server/store"
)

// p0.27: series tracking (`upcoming`), shared intro marks (`skip`) and
// recommendation shelves (`recs`). All one-shot.

const (
	maxTracked    = 40             // series looked at per `upcoming`
	scheduleTTL   = 24 * time.Hour // air dates are refreshed once a day
	recsBecause   = 3              // "çünkü X'i izledin" shelves
	recsShelfSize = 8
	recsListSize  = 20
)

// account checks auth on an account server; open servers have no lists.
func (h *Hub) account(auth string) (store.User, map[string]any) {
	st := h.cfg.Store
	if st == nil {
		return store.User{}, errMsg("no_accounts")
	}
	if auth == "" {
		return store.User{}, errMsg("auth_required")
	}
	u, err := st.UserByToken(auth)
	if err != nil {
		return store.User{}, errMsg(errCode(err))
	}
	return u, nil
}

type upcomingItem struct {
	Ref     string `json:"ref"`
	Title   string `json:"title"`
	Poster  string `json:"poster,omitempty"`
	Season  int    `json:"season,omitempty"` // next episode; empty when none is announced
	Episode int    `json:"episode,omitempty"`
	Name    string `json:"name,omitempty"`
	Airs    string `json:"airs,omitempty"` // YYYY-MM-DD
	Unseen  int    `json:"unseen"`         // aired episodes not marked seen
}

// Upcoming (p0.27): the next episodes of the series on the account's want and
// progress lists, plus the series it watches with a team.
func (h *Hub) Upcoming(ctx context.Context, in Inbound) map[string]any {
	u, e := h.account(in.Auth)
	if e != nil {
		return e
	}
	st := h.cfg.Store
	want, err := st.List(u.ID, store.ListWant)
	if err != nil {
		return errMsg("server_error")
	}
	prog, err := st.ProgressList(u.ID)
	if err != nil {
		return errMsg("server_error")
	}
	var refs []string
	byRef := map[string]store.Entry{}
	track := func(ref, title, poster string) {
		if _, ok := byRef[ref]; ok || len(refs) == maxTracked {
			return
		}
		refs = append(refs, ref)
		byRef[ref] = store.Entry{Ref: ref, Title: title, Poster: poster}
	}
	for _, p := range prog {
		if p.Kind == "series" {
			track(p.Ref, p.Title, p.Poster)
		}
	}
	for _, w := range want {
		if w.Kind == "series" {
			track(w.Ref, w.Title, w.Poster)
		}
	}
	info, err := st.SeriesInfo(refs)
	if err != nil {
		return errMsg("server_error")
	}
	h.refreshSchedules(ctx, refs, info)
	eps, err := st.Episodes(u.ID)
	if err != nil {
		return errMsg("server_error")
	}
	items := []upcomingItem{}
	for _, ref := range refs {
		x, ok := info[ref]
		if !ok {
			continue
		}
		seen := 0
		for _, ep := range eps[ref] {
			if !strings.HasPrefix(ep, "0:") {
				seen++
			}
		}
		it := upcomingItem{Ref: ref, Title: byRef[ref].Title, Poster: byRef[ref].Poster, Unseen: max(0, x.Aired-seen)}
		if it.Title == "" {
			it.Title = x.Title
		}
		if it.Poster == "" {
			it.Poster = x.Poster
		}
		if x.Episode > 0 {
			it.Season, it.Episode, it.Name, it.Airs = x.Season, x.Episode, x.Name, x.Airs
		}
		if it.Airs != "" || it.Unseen > 0 {
			items = append(items, it)
		}
	}
	slices.SortStableFunc(items, func(a, b upcomingItem) int {
		switch {
		case a.Airs == b.Airs:
			return 0
		case a.Airs == "":
			return 1
		case b.Airs == "":
			return -1
		}
		return strings.Compare(a.Airs, b.Airs)
	})
	team, err := st.TeamProgress(u.ID)
	if err != nil {
		return errMsg("server_error")
	}
	return map[string]any{"t": "upcoming", "items": items, "team": team}
}

// refreshSchedules fetches the air dates of refs missing from info or older than a
// day, a few at a time, and stores them. Failures keep the old row.
func (h *Hub) refreshSchedules(ctx context.Context, refs []string, info map[string]store.Series) {
	d, st := h.cfg.Discover, h.cfg.Store
	if d == nil {
		return
	}
	now := h.cfg.Now().UnixMilli()
	var stale []string
	for _, ref := range refs {
		if x, ok := info[ref]; !ok || now-x.Checked > scheduleTTL.Milliseconds() {
			stale = append(stale, ref)
		}
	}
	if len(stale) == 0 {
		return
	}
	ctx, cancel := context.WithTimeout(ctx, upstreamTimeout)
	defer cancel()
	got := make([]*store.Series, len(stale))
	sem := make(chan struct{}, 6)
	var wg sync.WaitGroup
	for i, ref := range stale {
		wg.Add(1)
		go func() {
			defer wg.Done()
			sem <- struct{}{}
			defer func() { <-sem }()
			_, id, err := resolveRef(ctx, d, ref, discover.KindTV)
			if err != nil {
				return
			}
			s, err := d.Schedule(ctx, id)
			if err != nil {
				return
			}
			x := store.Series{Ref: ref, Title: s.Title, Poster: s.Poster, Aired: s.Aired, Checked: now}
			if n := s.Next; n != nil {
				x.Season, x.Episode, x.Name, x.Airs = n.Season, n.Episode, n.Name, n.Airs
			}
			got[i] = &x
		}()
	}
	wg.Wait()
	for _, x := range got {
		if x != nil && st.PutSeries(*x) == nil {
			info[x.Ref] = *x
		}
	}
}

// Skip (p0.27): intro / recap marks of a series, shared by everyone on the server;
// the last writer wins. Every answer carries all marks of ref.
func (h *Hub) Skip(in Inbound) map[string]any {
	u, e := h.account(in.Auth)
	if e != nil {
		return e
	}
	st := h.cfg.Store
	ref := strings.TrimSpace(in.Ref)
	if ref == "" || len(ref) > 64 {
		return errMsg("bad_entry")
	}
	var err error
	switch in.Op {
	case "set":
		err = st.SetSkip(u.ID, ref, store.Skip{Season: in.Season, IntroStart: in.IntroStart, IntroEnd: in.IntroEnd, RecapEnd: in.RecapEnd})
	case "clear":
		err = st.ClearSkip(ref, in.Season)
	case "get", "":
	default:
		return errMsg("bad_message")
	}
	if err != nil {
		return errMsg(errCode(err))
	}
	marks, err := st.Skips(ref)
	if err != nil {
		return errMsg("server_error")
	}
	return map[string]any{"t": "skip", "ref": ref, "marks": marks}
}

type becauseShelf struct {
	Ref   string          `json:"ref"`
	Title string          `json:"title"`
	Items []discover.Item `json:"items"`
}

type togetherItem struct {
	store.Entry
	Mates []store.Holder `json:"mates"`
}

// Recs (p0.27): "çünkü X'i izledin" (similar to the newest 4–5 star seen titles),
// "arkadaşların beğendi" (mutual friends' 4–5 stars you haven't seen) and
// "… de izleyecek" (your want titles a mutual friend wants too).
func (h *Hub) Recs(ctx context.Context, in Inbound) map[string]any {
	u, e := h.account(in.Auth)
	if e != nil {
		return e
	}
	st := h.cfg.Store
	seen, err := st.List(u.ID, store.ListSeen)
	if err != nil {
		return errMsg("server_error")
	}
	want, err := st.List(u.ID, store.ListWant)
	if err != nil {
		return errMsg("server_error")
	}
	have := map[string]bool{}
	for _, l := range [][]store.Entry{seen, want} {
		for _, x := range l {
			have[x.Ref] = true
		}
	}

	because := []becauseShelf{}
	if d := h.cfg.Discover; d != nil {
		var liked []store.Entry
		for _, x := range seen {
			if x.Stars >= 4 && len(liked) < recsBecause {
				liked = append(liked, x)
			}
		}
		shelves := make([]becauseShelf, len(liked))
		ctx, cancel := context.WithTimeout(ctx, upstreamTimeout)
		defer cancel()
		var wg sync.WaitGroup
		for i, x := range liked {
			wg.Add(1)
			go func() {
				defer wg.Done()
				kind, id, err := resolveRef(ctx, d, x.Ref, x.Kind)
				if err != nil {
					return
				}
				sim, err := d.Similar(ctx, kind, id, 20)
				if err != nil {
					return
				}
				sh := becauseShelf{Ref: x.Ref, Title: x.Title, Items: []discover.Item{}}
				for _, it := range sim {
					ref := "tmdb:" + kind + ":" + strconv.Itoa(it.ID)
					if !have[ref] && (kind != discover.KindTV || !have["tmdb:series:"+strconv.Itoa(it.ID)]) && len(sh.Items) < recsShelfSize {
						sh.Items = append(sh.Items, it)
					}
				}
				shelves[i] = sh
			}()
		}
		wg.Wait()
		for _, sh := range shelves {
			if len(sh.Items) > 0 {
				because = append(because, sh)
			}
		}
	}

	friends, err := st.FriendsLiked(u.ID, recsListSize)
	if err != nil {
		return errMsg("server_error")
	}
	mates, err := st.WantMates(u.ID)
	if err != nil {
		return errMsg("server_error")
	}
	together := []togetherItem{}
	for _, x := range want {
		if m := mates[x.Ref]; len(m) > 0 && len(together) < recsListSize {
			together = append(together, togetherItem{Entry: x, Mates: m})
		}
	}
	return map[string]any{"t": "recs", "because": because, "friends": friends, "together": together}
}
