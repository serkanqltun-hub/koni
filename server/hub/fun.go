package hub

import (
	"context"
	"crypto/rand"
	"errors"
	"fmt"
	"maps"
	"math/big"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/serkanqltun-hub/koni/server/discover"
	"github.com/serkanqltun-hub/koni/server/subs"
)

// "Ne izlesek?" (docs/design/discovery.md §7.3) and room subtitles (media.md §6.2).
// Upstream calls run outside the room goroutine; their results come back via r.do.

const (
	maxProposals    = 12
	maxOwnProposals = 2
	upstreamTimeout = 20 * time.Second
)

type deckState struct {
	id    string
	mood  string // "🌃 Cyberpunk"
	by    string
	items []discover.Item
	votes map[string]map[int]bool // member id → TMDB id → like
	match int                     // TMDB id, 0 = none yet
	over  bool                    // everyone voted without a match: top cards went to the wheel
}

type proposal struct {
	ID     string        `json:"id"`
	By     string        `json:"by,omitempty"` // member id; "" = from a deck without a match
	ByName string        `json:"byName"`
	Item   discover.Item `json:"item"`
	user   int64
}

// wheelResult: the client turns the wheel clockwise by Turns full turns in SpinMs;
// slice i covers [i/n, (i+1)/n) of a turn clockwise from the top pointer, so the
// pointer stops inside slice Index (docs/PROTOCOL.md §4.13).
type wheelResult struct {
	Winner string  `json:"winner"`
	Index  int     `json:"index"`
	SpinMs int     `json:"spinMs"`
	Turns  float64 `json:"turns"`
	At     int64   `json:"at"` // server ms at spin start: late joiners skip the animation
}

type wheelState struct {
	proposals []proposal
	result    *wheelResult
	next      int
	pending   map[string]int // member id → title searches in flight
}

type roomSubtitle struct {
	media *Media // subtitles belong to the media they were picked for
	msg   []byte
}

// mayControl: what affects everyone (new deck, spin, room subtitle) follows the policy.
func (r *Room) mayControl(m *Member) bool {
	return r.policy != PolicyHostOnly || m.canControl()
}

func (r *Room) funError(m *Member, text string) {
	r.sendTo(m, enc(map[string]any{"t": "fun_error", "message": text}))
}

func (r *Room) activeMembers() []*Member {
	var out []*Member
	for _, id := range r.order {
		if m := r.members[id]; !m.ghost && !m.closed {
			out = append(out, m)
		}
	}
	return out
}

func (r *Room) presentMembers() []*Member {
	var out []*Member
	for _, id := range r.order {
		if m := r.members[id]; m != nil && !m.closed {
			out = append(out, m)
		}
	}
	return out
}

// sendFunState brings a joining (or resuming) member up to date.
func (r *Room) sendFunState(m *Member) {
	if d := r.cfg.Discover; d != nil {
		r.sendTo(m, enc(map[string]any{"t": "moods", "moods": d.Moods()}))
	}
	if r.deck != nil {
		r.sendTo(m, r.deckMsg(m))
	}
	if len(r.wheel.proposals) > 0 {
		r.sendTo(m, r.wheelMsg())
	}
	if r.night != nil {
		r.sendTo(m, r.nightMsg(m))
	}
	if s := r.subtitle; s != nil && s.media == r.media {
		r.sendTo(m, s.msg)
	}
	if len(r.queue.items) > 0 {
		r.sendTo(m, r.queueMsg())
	}
}

// ---- deck / swipe -----------------------------------------------------

func (r *Room) handleDiscover(m *Member, in Inbound) {
	d := r.cfg.Discover
	switch {
	case d == nil:
		r.funError(m, "Bu sunucuda keşif kapalı (TMDB anahtarı yok)")
		return
	case !r.mayControl(m):
		r.funError(m, "Yalnızca host ve yardımcı hostlar yeni deste açabilir")
		return
	case r.deckLoading:
		return
	case !r.allowRate(m):
		return
	}
	label := d.MoodLabel(in.Mood)
	if label == "" {
		r.funError(m, "Bilinmeyen mood")
		return
	}
	mood, maxRt, seen := in.Mood, min(max(in.MaxRuntime, 0), 400), maps.Clone(r.seen)
	r.deckLoading = true
	r.broadcast(enc(map[string]any{"t": "deck_loading", "mood": label, "by": m.Name}))
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), upstreamTimeout)
		defer cancel()
		items, err := d.Deck(ctx, mood, maxRt, seen)
		r.do(func() {
			r.deckLoading = false
			if err != nil {
				msg := err.Error()
				if errors.Is(err, discover.ErrNotFound) {
					msg = "Bu mood için yeni film kalmadı"
				}
				r.broadcast(enc(map[string]any{"t": "discover_failed", "message": msg}))
				return
			}
			r.deckSeq++
			r.deck = &deckState{id: fmt.Sprintf("k%d", r.deckSeq), mood: label, by: m.Name, items: items, votes: map[string]map[int]bool{}}
			for _, it := range items {
				r.seen[it.ID] = true
			}
			r.broadcast(r.deckMsg(nil))
			r.activity("discover", m.ID, fmt.Sprintf("%s yeni deste açtı: %s", m.Name, label))
		})
	}()
}

// deckMsg: for one member (mine = their own votes), or for everyone (nil).
func (r *Room) deckMsg(m *Member) []byte {
	dk := r.deck
	msg := map[string]any{"t": "deck", "deckId": dk.id, "mood": dk.mood, "by": dk.by, "items": dk.items, "over": dk.over}
	done, of := r.deckProgress()
	msg["progress"] = map[string]int{"done": done, "of": of}
	if dk.match != 0 {
		msg["match"] = dk.match
	}
	if m != nil {
		mine := map[string]bool{}
		for id, like := range dk.votes[m.ID] {
			mine[fmt.Sprint(id)] = like
		}
		msg["mine"] = mine
	}
	return enc(msg)
}

// deckProgress: how many room members voted on every card. Votes stay secret.
func (r *Room) deckProgress() (done, of int) {
	present := r.presentMembers()
	for _, a := range present {
		if len(r.deck.votes[a.ID]) >= len(r.deck.items) {
			done++
		}
	}
	return done, len(present)
}

func (r *Room) handleSwipe(m *Member, in Inbound) {
	dk := r.deck
	if dk == nil || in.DeckID != dk.id || dk.match != 0 || dk.over || (in.Vote != "like" && in.Vote != "pass") {
		return // stale or finished deck: nothing to do
	}
	found := false
	for _, it := range dk.items {
		found = found || it.ID == in.ItemID
	}
	if !found {
		return
	}
	v := dk.votes[m.ID]
	if v == nil {
		v = map[int]bool{}
		dk.votes[m.ID] = v
	}
	if _, voted := v[in.ItemID]; voted {
		return // votes are final
	}
	like := in.Vote == "like"
	v[in.ItemID] = like
	r.record("vote", m, map[string]any{"deck": dk.id, "tmdb": in.ItemID, "like": like})
	r.afterVotes()
}

// afterVotes runs after a vote or when the set of connected members changes: the
// first card every connected member liked is the match; if everyone finished the deck
// without one, the most liked three cards go to the wheel.
func (r *Room) afterVotes() {
	dk := r.deck
	if dk == nil || dk.match != 0 || dk.over {
		return
	}
	present := r.presentMembers()
	if len(present) == 0 {
		return
	}
	for _, it := range dk.items {
		all := true
		for _, a := range present {
			if !dk.votes[a.ID][it.ID] {
				all = false
				break
			}
		}
		if all {
			for _, v := range dk.votes {
				if like, voted := v[it.ID]; voted && !like {
					all = false
					break
				}
			}
		}
		if all {
			dk.match = it.ID
			r.broadcast(enc(map[string]any{"t": "match", "deckId": dk.id, "item": it}))
			r.activity("match", "", "🎉 Eşleşme: "+it.Title)
			r.nightDeckOver(dk.id)
			return
		}
	}
	done, of := r.deckProgress()
	dk.over = done == of
	r.broadcast(enc(map[string]any{"t": "deck_progress", "deckId": dk.id, "done": done, "of": of, "over": dk.over}))
	if dk.over {
		r.deckToWheel()
		r.nightDeckOver(dk.id)
	}
}

func (r *Room) deckToWheel() {
	dk := r.deck
	likes := map[int]int{}
	for _, v := range dk.votes {
		for id, like := range v {
			if like {
				likes[id]++
			}
		}
	}
	ranked := append([]discover.Item(nil), dk.items...)
	// stable: ties keep deck order
	for i := 1; i < len(ranked); i++ {
		for j := i; j > 0 && likes[ranked[j].ID] > likes[ranked[j-1].ID]; j-- {
			ranked[j], ranked[j-1] = ranked[j-1], ranked[j]
		}
	}
	if r.wheel.result != nil {
		r.resetWheel()
	}
	keep := r.wheel.proposals[:0]
	for _, p := range r.wheel.proposals {
		if p.By != "" {
			keep = append(keep, p)
		}
	}
	r.wheel.proposals = keep
	for _, it := range ranked[:min(3, len(ranked))] {
		if len(r.wheel.proposals) < maxProposals && !r.onWheel(it) {
			r.wheel.next++
			r.wheel.proposals = append(r.wheel.proposals, proposal{ID: fmt.Sprintf("p%d", r.wheel.next), ByName: "Deste", Item: it})
		}
	}
	r.broadcast(r.wheelMsg())
	r.feed("discover", "", "Eşleşme yok: en çok beğenilen filmler çarka eklendi")
}

// ---- wheel ------------------------------------------------------------

func (r *Room) wheelMsg() []byte {
	ps := r.wheel.proposals
	if ps == nil {
		ps = []proposal{}
	}
	msg := map[string]any{"t": "wheel", "proposals": ps}
	if r.wheel.result != nil {
		msg["result"] = r.wheel.result
	}
	return enc(msg)
}

func (r *Room) resetWheel() {
	r.wheel = wheelState{next: r.wheel.next, pending: r.wheel.pending}
}

func (r *Room) onWheel(it discover.Item) bool {
	for _, p := range r.wheel.proposals {
		if (it.ID != 0 && p.Item.ID == it.ID) || strings.EqualFold(p.Item.Title, it.Title) {
			return true
		}
	}
	return false
}

func (r *Room) handlePropose(m *Member, in Inbound) {
	if r.wheel.result != nil { // a finished spin: the next proposal starts a new round
		r.resetWheel()
		r.broadcast(r.wheelMsg())
	}
	own := r.wheel.pending[m.ID]
	for _, p := range r.wheel.proposals {
		if p.By == m.ID {
			own++
		}
	}
	switch {
	case own >= maxOwnProposals:
		r.funError(m, fmt.Sprintf("En fazla %d öneri ekleyebilirsin", maxOwnProposals))
		return
	case len(r.wheel.proposals) >= maxProposals:
		r.funError(m, "Çark dolu")
		return
	}
	if in.ItemID != 0 { // a card from the current deck
		if r.deck != nil {
			for _, it := range r.deck.items {
				if it.ID == in.ItemID {
					r.addProposal(m, it)
					return
				}
			}
		}
		r.funError(m, "Bu film destede yok")
		return
	}
	q := strings.TrimSpace(in.Query)
	if n := utf8.RuneCountInString(q); n == 0 || n > 80 {
		r.funError(m, "Film adı 1–80 karakter olmalı")
		return
	}
	d := r.cfg.Discover
	if d == nil {
		r.addProposal(m, discover.Item{Title: q})
		return
	}
	if !r.allowRate(m) {
		return
	}
	if r.wheel.pending == nil {
		r.wheel.pending = map[string]int{}
	}
	r.wheel.pending[m.ID]++
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), upstreamTimeout)
		defer cancel()
		it, err := d.Search(ctx, q)
		if err != nil {
			it = discover.Item{Title: q} // still a valid proposal, just without a card
		}
		r.do(func() {
			r.wheel.pending[m.ID]--
			if !m.closed {
				r.addProposal(m, it)
			}
		})
	}()
}

func (r *Room) addProposal(m *Member, it discover.Item) {
	if r.wheel.result != nil {
		r.resetWheel()
	}
	if r.onWheel(it) {
		r.funError(m, "Bu film zaten çarkta")
		return
	}
	if len(r.wheel.proposals) >= maxProposals {
		r.funError(m, "Çark dolu")
		return
	}
	r.wheel.next++
	r.wheel.proposals = append(r.wheel.proposals, proposal{ID: fmt.Sprintf("p%d", r.wheel.next), By: m.ID, ByName: m.Name, Item: it, user: m.User})
	r.broadcast(r.wheelMsg())
	r.feed("propose", m.ID, fmt.Sprintf("%s çarka ekledi: %s", m.Name, it.Title))
}

func (r *Room) handleUnpropose(m *Member, in Inbound) {
	if r.wheel.result != nil {
		return
	}
	for i, p := range r.wheel.proposals {
		if p.ID == in.ProposalID && (p.By == m.ID || m.canControl()) {
			r.wheel.proposals = append(r.wheel.proposals[:i], r.wheel.proposals[i+1:]...)
			r.broadcast(r.wheelMsg())
			return
		}
	}
}

func (r *Room) handleSpin(m *Member) {
	ps := r.wheel.proposals
	switch {
	case !r.mayControl(m):
		r.funError(m, "Çarkı yalnızca host ve yardımcı hostlar çevirebilir")
		return
	case r.wheel.result != nil:
		return
	case len(ps) < 2:
		r.funError(m, "Çark için en az 2 öneri gerekli")
		return
	}
	n := len(ps)
	idx := cryptoInt(n)
	u := 0.15 + 0.7*float64(cryptoInt(1000))/1000 // stay off the slice edges
	res := &wheelResult{
		Winner: ps[idx].ID, Index: idx, SpinMs: 4200 + cryptoInt(800),
		Turns: float64(5+cryptoInt(3)) + 1 - (float64(idx)+u)/float64(n), At: r.nowMs(),
	}
	r.wheel.result = res
	r.broadcast(r.wheelMsg())
	for i, p := range ps {
		if p.user != 0 {
			r.recordUser("proposal", p.user, map[string]any{"title": p.Item.Title, "tmdb": p.Item.ID, "won": i == idx})
		}
	}
	title := ps[idx].Item.Title
	r.feed("spin", m.ID, m.Name+" çarkı çevirdi")
	time.AfterFunc(time.Duration(res.SpinMs)*time.Millisecond, func() {
		r.do(func() { r.activity("wheel", "", "🎡 Çark: "+title) })
	})
}

func (r *Room) handleWheelClear(m *Member) {
	if !r.mayControl(m) {
		r.funError(m, "Çarkı yalnızca host ve yardımcı hostlar temizleyebilir")
		return
	}
	r.resetWheel()
	r.broadcast(r.wheelMsg())
}

func cryptoInt(n int) int {
	v, err := rand.Int(rand.Reader, big.NewInt(int64(n)))
	if err != nil {
		panic(err)
	}
	return int(v.Int64())
}

// ---- subtitles --------------------------------------------------------

func (r *Room) handleSubsSearch(m *Member, in Inbound) {
	sc := r.cfg.Subs
	if sc == nil {
		r.sendTo(m, enc(map[string]any{"t": "subs_results", "lang": in.Lang, "error": "Bu sunucuda altyazı araması kapalı (OpenSubtitles anahtarı yok)"}))
		return
	}
	if !r.allowRate(m) {
		return
	}
	q := subs.Query{Hash: in.Hash, IMDb: in.IMDb, Text: in.Query, Langs: strings.Split(in.Lang, ",")}
	if q.Text == "" && r.media != nil {
		q.Text = r.media.Title
	}
	lang := in.Lang
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), upstreamTimeout)
		defer cancel()
		res, err := sc.Search(ctx, q)
		msg := map[string]any{"t": "subs_results", "lang": lang, "items": res}
		if err != nil {
			msg["error"] = err.Error()
		}
		r.do(func() { r.sendTo(m, enc(msg)) })
	}()
}

func (r *Room) handleSubsPick(m *Member, in Inbound) {
	sc := r.cfg.Subs
	switch {
	case sc == nil:
		r.funError(m, "Bu sunucuda altyazı araması kapalı")
		return
	case !r.mayControl(m):
		r.funError(m, "Odaya altyazıyı yalnızca host ve yardımcı hostlar ekleyebilir")
		return
	case r.subsBusy || in.FileID <= 0:
		return
	}
	r.subsBusy = true
	media, id, lang := r.media, in.FileID, in.Lang
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), upstreamTimeout)
		defer cancel()
		name, text, err := sc.Download(ctx, id)
		r.do(func() {
			r.subsBusy = false
			if err != nil {
				r.funError(m, err.Error())
				return
			}
			if r.media != media {
				return // the room moved on to other media meanwhile
			}
			msg := enc(map[string]any{"t": "subtitle", "name": name, "lang": lang, "text": text, "by": m.Name})
			r.subtitle = &roomSubtitle{media: media, msg: msg}
			r.broadcast(msg)
			r.activity("subtitle", m.ID, fmt.Sprintf("%s odaya altyazı ekledi: %s", m.Name, name))
		})
	}()
}
