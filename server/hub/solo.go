package hub

import (
	"context"
	"strconv"
	"strings"

	"github.com/serkanqltun-hub/koni/server/discover"
	"github.com/serkanqltun-hub/koni/server/store"
)

// maxWatchSec caps one solo watch report: the client reports about once a minute.
const maxWatchSec = 120

// Watch records solo watching (p0.25 `watch`): a `watch` event like a room's, with
// solo:true and room "solo:<account>". A report can't count more time than has passed
// since the account's previous one.
func (h *Hub) Watch(in Inbound) map[string]any {
	st := h.cfg.Store
	if st == nil {
		return errMsg("no_accounts")
	}
	if in.Auth == "" {
		return errMsg("auth_required")
	}
	u, err := st.UserByToken(in.Auth)
	if err != nil {
		return errMsg(errCode(err))
	}
	now := h.cfg.Now().UnixMilli()
	sec := int64(min(max(in.Sec, 0), maxWatchSec))
	h.watchMu.Lock()
	if last, ok := h.watchAt[u.ID]; ok {
		sec = min(sec, max(0, (now-last)/1000))
	}
	if sec > 0 {
		h.watchAt[u.ID] = now
	}
	h.watchMu.Unlock()
	if sec > 0 {
		ms := sec * 1000
		ev := map[string]any{"ms": ms, "night": store.NightMs(now-ms, now), "solo": true}
		if t := []rune(strings.TrimSpace(in.Title)); len(t) > 0 {
			ev["title"] = string(t[:min(len(t), 200)])
		}
		st.Record("solo:"+strconv.FormatInt(u.ID, 10), u.ID, "watch", ev)
	}
	return map[string]any{"t": "watch", "sec": sec}
}

// similarSize: how many "Benzerleri" a detail carries.
const similarSize = 10

// Detail is night_detail without a room (p0.25 `detail`): the full card for ref
// ("tt…" or "tmdb:<movie|tv>:<id>") plus similar titles.
func (h *Hub) Detail(ctx context.Context, in Inbound) map[string]any {
	if st := h.cfg.Store; st != nil {
		if in.Auth == "" {
			return errMsg("auth_required")
		}
		if _, err := st.UserByToken(in.Auth); err != nil {
			return errMsg(errCode(err))
		}
	}
	d := h.cfg.Discover
	if d == nil {
		return map[string]any{"t": "detail", "ref": in.Ref, "error": "Bu sunucuda künye bilgisi kapalı"}
	}
	ctx, cancel := context.WithTimeout(ctx, upstreamTimeout)
	defer cancel()
	kind, id, err := resolveRef(ctx, d, in.Ref, in.Kind)
	var it discover.Item
	if err == nil {
		it, err = d.Detail(ctx, kind, id)
	}
	if err != nil {
		return map[string]any{"t": "detail", "ref": in.Ref, "error": err.Error()}
	}
	similar, err := d.Similar(ctx, kind, id, similarSize)
	if err != nil {
		similar = []discover.Item{}
	}
	return map[string]any{"t": "detail", "ref": in.Ref, "item": it, "similar": similar}
}

// resolveRef turns a list ref into a TMDB kind and id. kind ("movie", "series"/"tv")
// helps an IMDb id; without it a movie is tried first.
func resolveRef(ctx context.Context, d *discover.Client, ref, kind string) (string, int, error) {
	if kind == "series" {
		kind = discover.KindTV
	}
	if rest, ok := strings.CutPrefix(ref, "tmdb:"); ok {
		k, n, _ := strings.Cut(rest, ":")
		if k == "series" {
			k = discover.KindTV
		}
		id, err := strconv.Atoi(n)
		if !discover.ValidKind(k) || err != nil || id <= 0 {
			return "", 0, discover.ErrNotFound
		}
		return k, id, nil
	}
	if !strings.HasPrefix(ref, "tt") || len(ref) > 16 {
		return "", 0, discover.ErrNotFound
	}
	if discover.ValidKind(kind) {
		id, err := d.FindIMDb(ctx, kind, ref)
		return kind, id, err
	}
	for _, k := range []string{discover.KindMovie, discover.KindTV} {
		if id, err := d.FindIMDb(ctx, k, ref); err == nil {
			return k, id, nil
		}
	}
	return "", 0, discover.ErrNotFound
}

// soloAccount checks auth like `detail`: required on an account server. It returns
// the account id (0 on an open server) or an error code.
func (h *Hub) soloAccount(auth string) (int64, string) {
	st := h.cfg.Store
	if st == nil {
		return 0, ""
	}
	if auth == "" {
		return 0, "auth_required"
	}
	u, err := st.UserByToken(auth)
	if err != nil {
		return 0, errCode(err)
	}
	return u.ID, ""
}

// SoloNight is "Ne izlesem?" (p0.26 `solo_night`): night_suggest for one person. It
// deals up to DeckSize detailed cards for the filter, leaving out the account's seen
// list and titles longer than budgetMin; the rest of the pool waits for
// `solo_night_pick`. There is no vote: the client decides.
func (h *Hub) SoloNight(ctx context.Context, in Inbound) map[string]any {
	user, code := h.soloAccount(in.Auth)
	if code != "" {
		return errMsg(code)
	}
	kind := in.Kind
	if kind == "series" {
		kind = discover.KindTV
	}
	msg := map[string]any{"t": "solo_night", "kind": kind, "items": []discover.Item{}}
	if in.Dub != "" {
		msg["dub"] = in.Dub // not filtered here: the client applies it to sources
	}
	d := h.cfg.Discover
	switch {
	case d == nil:
		msg["error"] = "Bu sunucuda keşif kapalı"
		return msg
	case !discover.ValidKind(kind):
		msg["error"] = "Film ya da dizi seçilmeli"
		return msg
	}
	var f discover.Filter
	if in.Filter != nil {
		f = *in.Filter
	}
	f.HideSeen = false
	budget := max(in.BudgetMin, 0)
	if budget > 0 && (f.RuntimeMax == 0 || budget < f.RuntimeMax) {
		f.RuntimeMax = budget
	}
	seen := map[string]bool{}
	if st := h.cfg.Store; st != nil {
		list, _ := st.List(user, store.ListSeen)
		for _, e := range list {
			seen[e.Ref] = true
		}
	}
	ctx, cancel := context.WithTimeout(ctx, upstreamTimeout)
	defer cancel()
	pool, _, err := d.Suggest(ctx, kind, f, nil)
	if err != nil {
		msg["error"] = err.Error()
		return msg
	}
	var cards []discover.Item
	for _, it := range d.Fill(ctx, pool) {
		if !seenCard(seen, kind, it) && (budget == 0 || it.Runtime <= budget) {
			cards = append(cards, it)
		}
	}
	deal := cards[:min(len(cards), discover.DeckSize)]
	rest := cards[len(deal):]
	h.soloMu.Lock()
	h.soloRest[user] = rest
	h.soloMu.Unlock()
	if len(deal) > 0 {
		msg["items"] = deal
	}
	msg["more"] = len(rest)
	return msg
}

// SoloNightPick (p0.26 `solo_night_pick`): the user is done with card ref (liked or
// passed); the answer carries the next card of the last solo_night pool, if any.
func (h *Hub) SoloNightPick(in Inbound) map[string]any {
	user, code := h.soloAccount(in.Auth)
	if code != "" {
		return errMsg(code)
	}
	msg := map[string]any{"t": "solo_night_pick", "ref": in.Ref}
	h.soloMu.Lock()
	defer h.soloMu.Unlock()
	rest := h.soloRest[user]
	if len(rest) == 0 {
		delete(h.soloRest, user)
		msg["more"] = 0
		return msg
	}
	msg["item"], h.soloRest[user] = rest[0], rest[1:]
	msg["more"] = len(rest) - 1
	return msg
}

// seenCard: a card is on the seen list by IMDb id or by its tmdb: ref.
func seenCard(seen map[string]bool, kind string, it discover.Item) bool {
	id := strconv.Itoa(it.ID)
	if it.IMDbID != "" && seen[it.IMDbID] {
		return true
	}
	if kind == discover.KindTV {
		return seen["tmdb:tv:"+id] || seen["tmdb:series:"+id]
	}
	return seen["tmdb:movie:"+id]
}

// Browse is "Göz at" (p0.26 `browse`): one page of 30 titles for the filters,
// without details (the card opens with `detail`).
func (h *Hub) Browse(ctx context.Context, in Inbound) map[string]any {
	if _, code := h.soloAccount(in.Auth); code != "" {
		return errMsg(code)
	}
	kind := in.Kind
	if kind == "series" {
		kind = discover.KindTV
	}
	page := max(in.Page, 1)
	msg := map[string]any{"t": "browse", "kind": kind, "page": page, "items": []discover.Item{}, "total": 0}
	d := h.cfg.Discover
	switch {
	case d == nil:
		msg["error"] = "Bu sunucuda keşif kapalı"
		return msg
	case !discover.ValidKind(kind):
		msg["error"] = "Film ya da dizi seçilmeli"
		return msg
	}
	lang := strings.ToLower(strings.TrimSpace(in.Lang))
	if len(lang) != 2 {
		lang = ""
	}
	ctx, cancel := context.WithTimeout(ctx, upstreamTimeout)
	defer cancel()
	items, total, err := d.Browse(ctx, kind, discover.BrowseQuery{
		Genre: in.Genre, YearFrom: in.YearFrom, YearTo: in.YearTo, MinRating: in.MinRating,
		MaxMin: in.MaxMin, Lang: lang, Sort: in.Sort, Page: page,
	})
	if err != nil {
		msg["error"] = err.Error()
		return msg
	}
	msg["items"], msg["total"] = items, total
	return msg
}
