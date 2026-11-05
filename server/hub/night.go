package hub

import (
	"context"
	"fmt"
	"maps"
	"math/rand/v2"
	"slices"
	"strings"
	"sync"
	"unicode/utf8"

	"github.com/serkanqltun-hub/koni/server/discover"
	"github.com/serkanqltun-hub/koni/server/store"
)

// "Ne izlesek?" v2 (docs/PROTOCOL.md §3.9a, p0.17). The host locks film or series and
// how many titles each member may propose. Then: setup (everyone picks a mood or
// filter and says ready) → pick (everyone proposes up to perUser titles into an
// anonymous pool) → vote (the pool becomes a deck; swipe/match/wheel as before) → done.

const (
	nightSetup = "setup"
	nightPick  = "pick"
	nightVote  = "vote"
	nightDone  = "done"

	maxPerUser  = 5
	maxPreviews = 8
	maxListPool = 20 // pool:"lists" deals at most this many titles

	poolLists = "lists"
)

type nightState struct {
	id      string
	kind    string // discover.KindMovie | KindTV
	perUser int
	by      string // starter's name
	phase   string
	ready   map[string]string // member id → mood label; present = ready
	picks   map[string][]discover.Item
	pending map[string]int // detail fetches in flight per member
	done    map[string]bool
	deckID  string
	source  string // poolLists: the pool is being filled from want lists (p0.26)
}

func (r *Room) handleNightStart(m *Member, in Inbound) {
	switch {
	case r.cfg.Discover == nil:
		r.funError(m, "Bu sunucuda keşif kapalı (TMDB anahtarı yok)")
		return
	case !r.mayControl(m):
		r.funError(m, "Ne izlesek?'i yalnızca host ve yardımcı hostlar başlatabilir")
		return
	case !discover.ValidKind(in.Kind):
		r.funError(m, "Film ya da dizi seçilmeli")
		return
	}
	r.nightSeq++
	r.night = &nightState{
		id: fmt.Sprintf("n%d", r.nightSeq), kind: in.Kind, perUser: min(max(in.PerUser, 1), maxPerUser), by: m.Name,
		phase: nightSetup, ready: map[string]string{}, picks: map[string][]discover.Item{}, pending: map[string]int{}, done: map[string]bool{},
	}
	if in.Pool == poolLists {
		r.nightFromLists()
	}
	r.broadcastNight()
	what := "Film gecesi"
	if in.Kind == discover.KindTV {
		what = "Dizi gecesi"
	}
	r.activity("night", m.ID, fmt.Sprintf("%s Ne izlesek? başlattı: %s", m.Name, what))
}

// nightFromLists (p0.26 pool:"lists") fills the pool from the present members'
// "İzleyeceklerim": titles everyone wants, or if there are none, titles anyone wants
// (most wanted first). The deck is dealt as soon as the cards are fetched; with no
// listed titles the night goes on as usual (setup) after a fun_error.
func (r *Room) nightFromLists() {
	n, st := r.night, r.cfg.Store
	listKind := "movie"
	if n.kind == discover.KindTV {
		listKind = "series"
	}
	var refs []string // first seen order
	count := map[string]int{}
	users := 0
	if st != nil {
		for _, a := range r.presentMembers() {
			if a.User == 0 {
				continue
			}
			users++
			list, _ := st.List(a.User, store.ListWant)
			for _, e := range list {
				if e.Kind != listKind {
					continue
				}
				if count[e.Ref] == 0 {
					refs = append(refs, e.Ref)
				}
				count[e.Ref]++
			}
		}
	}
	var all []string
	for _, ref := range refs {
		if count[ref] == users {
			all = append(all, ref)
		}
	}
	if len(all) == 0 {
		all = slices.Clone(refs)
		slices.SortStableFunc(all, func(a, b string) int { return count[b] - count[a] })
	}
	all = all[:min(len(all), maxListPool)]
	if len(all) == 0 {
		r.broadcast(enc(map[string]any{"t": "fun_error", "message": "İzleyeceklerim listelerinde bu türde başlık yok: herkes kendi önerisini seçsin"}))
		return
	}
	n.source = poolLists
	d, kind := r.cfg.Discover, n.kind
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), upstreamTimeout)
		defer cancel()
		got := make([]discover.Item, len(all))
		var wg sync.WaitGroup
		for i, ref := range all {
			wg.Add(1)
			go func() {
				defer wg.Done()
				if k, id, err := resolveRef(ctx, d, ref, kind); err == nil && k == kind {
					got[i], _ = d.Detail(ctx, kind, id)
				}
			}()
		}
		wg.Wait()
		var pool []discover.Item
		in := map[int]bool{}
		for _, it := range got {
			if it.ID > 0 && it.Title != "" && !in[it.ID] {
				in[it.ID] = true
				pool = append(pool, it)
			}
		}
		r.do(func() {
			if r.night != n || n.source != poolLists {
				return // closed or restarted meanwhile
			}
			n.source = ""
			if len(pool) == 0 {
				r.broadcast(enc(map[string]any{"t": "fun_error", "message": "Listelerdeki başlıklar açılamadı: herkes kendi önerisini seçsin"}))
				r.broadcastNight()
				return
			}
			r.nightDeal(pool)
			r.broadcastNight()
		})
	}()
}

func (r *Room) handleNightClose(m *Member) {
	if r.night == nil {
		return
	}
	if !r.mayControl(m) {
		r.funError(m, "Ne izlesek?'i yalnızca host ve yardımcı hostlar kapatabilir")
		return
	}
	r.night = nil
	r.broadcast(enc(map[string]any{"t": "night", "closed": true}))
}

func (r *Room) handleNightReady(m *Member, in Inbound) {
	n := r.night
	if n == nil || n.phase != nightSetup {
		return
	}
	if in.Ready {
		label := strings.TrimSpace(in.Mood)
		if utf8.RuneCountInString(label) > 40 {
			label = string([]rune(label)[:40])
		}
		n.ready[m.ID] = label
	} else {
		delete(n.ready, m.ID)
	}
	r.nightAdvance()
	r.broadcastNight()
}

// nightLookup runs a discover call outside the room goroutine and sends its result
// to m only.
func (r *Room) nightLookup(m *Member, fn func(ctx context.Context, d *discover.Client) any) {
	d := r.cfg.Discover
	if d == nil || r.night == nil || !r.allowRate(m) {
		return
	}
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), upstreamTimeout)
		defer cancel()
		msg := fn(ctx, d)
		r.do(func() { r.sendTo(m, enc(msg)) })
	}()
}

func (r *Room) handleNightSuggest(m *Member, in Inbound) {
	if r.night == nil || in.Filter == nil {
		return
	}
	kind, f := r.night.kind, *in.Filter
	skip := map[int]bool{}
	if f.HideSeen {
		skip = maps.Clone(r.seen)
	}
	r.nightLookup(m, func(ctx context.Context, d *discover.Client) any {
		items, total, err := d.Suggest(ctx, kind, f, skip)
		msg := map[string]any{"t": "night_suggestions", "items": items, "total": total}
		if items == nil {
			msg["items"] = []discover.Item{}
		}
		if err != nil {
			msg["error"] = err.Error()
		}
		return msg
	})
}

func (r *Room) handleNightPreview(m *Member, in Inbound) {
	if r.night == nil || len(in.Filters) == 0 {
		return
	}
	kind, fs := r.night.kind, in.Filters[:min(len(in.Filters), maxPreviews)]
	r.nightLookup(m, func(ctx context.Context, d *discover.Client) any {
		return map[string]any{"t": "night_previews", "items": d.Previews(ctx, kind, fs)}
	})
}

func (r *Room) handleNightSearch(m *Member, in Inbound) {
	q := strings.TrimSpace(in.Query)
	if r.night == nil || q == "" || utf8.RuneCountInString(q) > 80 {
		return
	}
	kind := r.night.kind
	r.nightLookup(m, func(ctx context.Context, d *discover.Client) any {
		items, err := d.SearchList(ctx, kind, q)
		msg := map[string]any{"t": "night_results", "query": q, "items": items}
		if err != nil {
			msg["items"], msg["error"] = []discover.Item{}, err.Error()
		}
		return msg
	})
}

func (r *Room) handleNightDetail(m *Member, in Inbound) {
	if r.night == nil || in.ItemID <= 0 {
		return
	}
	kind, id := r.night.kind, in.ItemID
	r.nightLookup(m, func(ctx context.Context, d *discover.Client) any {
		it, err := d.Detail(ctx, kind, id)
		msg := map[string]any{"t": "night_detail", "item": it}
		if err != nil {
			msg["error"] = err.Error()
		}
		return msg
	})
}

func (r *Room) handleNightPick(m *Member, in Inbound) {
	n, d := r.night, r.cfg.Discover
	byIMDb := in.ItemID <= 0 && strings.HasPrefix(in.IMDb, "tt") && len(in.IMDb) <= 16 // from a profile list (p0.23)
	if n == nil || d == nil || n.phase != nightPick || n.done[m.ID] || (in.ItemID <= 0 && !byIMDb) {
		return
	}
	for _, it := range n.picks[m.ID] {
		if it.ID == in.ItemID {
			return
		}
	}
	if len(n.picks[m.ID])+n.pending[m.ID] >= n.perUser {
		r.funError(m, fmt.Sprintf("En fazla %d öneri seçebilirsin", n.perUser))
		return
	}
	if !r.allowRate(m) {
		return
	}
	n.pending[m.ID]++
	kind, id, imdb := n.kind, in.ItemID, in.IMDb
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), upstreamTimeout)
		defer cancel()
		var err error
		if byIMDb {
			id, err = d.FindIMDb(ctx, kind, imdb)
		}
		var it discover.Item
		if err == nil {
			it, err = d.Detail(ctx, kind, id)
		}
		r.do(func() {
			if r.night != n {
				return // closed or restarted meanwhile
			}
			n.pending[m.ID]--
			if err != nil {
				r.funError(m, "Bu başlık bulunamadı")
			} else if !n.done[m.ID] && n.phase == nightPick && !slices.ContainsFunc(n.picks[m.ID], func(x discover.Item) bool { return x.ID == it.ID }) {
				n.picks[m.ID] = append(n.picks[m.ID], it)
			}
			r.nightAdvance()
			r.broadcastNight()
		})
	}()
}

func (r *Room) handleNightUnpick(m *Member, in Inbound) {
	n := r.night
	if n == nil || n.phase != nightPick || n.done[m.ID] {
		return
	}
	ps := n.picks[m.ID]
	for i, it := range ps {
		if it.ID == in.ItemID {
			n.picks[m.ID] = append(ps[:i:i], ps[i+1:]...)
			r.broadcastNight()
			return
		}
	}
}

func (r *Room) handleNightDone(m *Member, in Inbound) {
	n := r.night
	if n == nil || n.phase != nightPick {
		return
	}
	if in.Ready {
		n.done[m.ID] = true
	} else {
		delete(n.done, m.ID)
	}
	r.nightAdvance()
	r.broadcastNight()
}

// nightAdvance moves to the next phase once every connected member is ready (setup)
// or has sent their proposals (pick). It also runs when members come and go.
func (r *Room) nightAdvance() {
	n := r.night
	if n == nil {
		return
	}
	present := r.presentMembers()
	if len(present) == 0 {
		return
	}
	all := func(ok func(id string) bool) bool {
		for _, a := range present {
			if !ok(a.ID) {
				return false
			}
		}
		return true
	}
	if n.source == poolLists {
		return // the pool is being filled from the lists
	}
	switch n.phase {
	case nightSetup:
		if all(func(id string) bool { _, ok := n.ready[id]; return ok }) {
			n.phase = nightPick
		}
	case nightPick:
		if !all(func(id string) bool { return n.done[id] && n.pending[id] == 0 }) {
			return
		}
		var pool []discover.Item
		in := map[int]bool{}
		for _, id := range r.order {
			for _, it := range n.picks[id] {
				if !in[it.ID] {
					in[it.ID] = true
					pool = append(pool, it)
				}
			}
		}
		if len(pool) == 0 {
			n.done = map[string]bool{}
			r.broadcast(enc(map[string]any{"t": "fun_error", "message": "Havuz boş: en az bir öneri seçin"}))
			return
		}
		r.nightDeal(pool)
	}
}

// nightDeal shuffles the pool into the vote deck.
func (r *Room) nightDeal(pool []discover.Item) {
	n := r.night
	rand.Shuffle(len(pool), func(i, j int) { pool[i], pool[j] = pool[j], pool[i] })
	r.deckSeq++
	r.deck = &deckState{id: fmt.Sprintf("k%d", r.deckSeq), mood: "✦ Ne izlesek?", by: n.by, items: pool, votes: map[string]map[int]bool{}}
	for _, it := range pool {
		r.seen[it.ID] = true
	}
	n.phase, n.deckID = nightVote, r.deck.id
	r.broadcast(r.deckMsg(nil))
	r.feed("night", "", fmt.Sprintf("Havuz hazır: %d başlık oylamada", len(pool)))
}

// nightDeckOver: the night's deck found a match or went to the wheel.
func (r *Room) nightDeckOver(deckID string) {
	if n := r.night; n != nil && n.deckID == deckID && n.phase == nightVote {
		n.phase = nightDone
		r.broadcastNight()
	}
}

type nightMember struct {
	ID    string `json:"id"`
	Name  string `json:"name"`
	Ready bool   `json:"ready"`
	Mood  string `json:"mood,omitempty"`
	Picks int    `json:"picks"`
	Done  bool   `json:"done"`
}

// nightMsg: the shared state plus m's own proposals (others' stay anonymous).
func (r *Room) nightMsg(m *Member) []byte {
	n := r.night
	pool := 0
	var members []nightMember
	for _, a := range r.presentMembers() {
		label, ready := n.ready[a.ID]
		members = append(members, nightMember{ID: a.ID, Name: a.Name, Ready: ready, Mood: label, Picks: len(n.picks[a.ID]), Done: n.done[a.ID]})
		pool += len(n.picks[a.ID])
	}
	mine := n.picks[m.ID]
	if mine == nil {
		mine = []discover.Item{}
	}
	msg := map[string]any{
		"t": "night", "id": n.id, "kind": n.kind, "perUser": n.perUser, "by": n.by, "phase": n.phase,
		"members": members, "pool": pool, "mine": mine, "pending": n.pending[m.ID],
	}
	if n.deckID != "" {
		msg["deckId"] = n.deckID
	}
	if n.source != "" {
		msg["source"] = n.source // p0.26: pool being filled from want lists
	}
	return enc(msg)
}

func (r *Room) broadcastNight() {
	if r.night == nil {
		return
	}
	for _, m := range r.members {
		r.sendTo(m, r.nightMsg(m))
	}
}

// nightMembersChanged: someone joined or left; counts and phases may change.
func (r *Room) nightMembersChanged() {
	if r.night != nil {
		r.nightAdvance()
		r.broadcastNight()
	}
}
