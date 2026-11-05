package hub

// p0.28 in the room: a break ("Mola ver", `cmd op:"break"` / `op:"ready"`) and the
// room history ("Son açılanlar": the last 20 media, who opened them, where they
// were left).

import (
	"fmt"
	"slices"
)

const (
	breakDefaultMin = 5
	breakMaxMin     = 30
	historySize     = 20
)

// breakState: the room paused for a break; it goes on when every connected member
// says ready.
type breakState struct {
	by    string
	min   int
	until int64 // unix ms; only shown, the room does not resume by itself
	ready map[string]bool
}

// histItem is one line of the room history, newest first.
type histItem struct {
	Media *Media  `json:"media"`
	By    string  `json:"by"`
	At    int64   `json:"at"`  // opened (unix ms)
	Pos   float64 `json:"pos"` // where the room left it (s); live for the current media
}

// breakWaiters: connected members the break still waits for.
func (r *Room) breakWaiters() (ready, waiting []string) {
	ready, waiting = []string{}, []string{}
	for _, id := range r.order {
		m := r.members[id]
		if m == nil || m.closed || m.ghost || m.detached {
			continue
		}
		if r.brk.ready[id] {
			ready = append(ready, id)
		} else {
			waiting = append(waiting, id)
		}
	}
	return ready, waiting
}

func (r *Room) breakMsg() map[string]any {
	if r.brk == nil {
		return nil
	}
	ready, waiting := r.breakWaiters()
	return map[string]any{"t": "break", "by": r.brk.by, "min": r.brk.min, "until": r.brk.until, "ready": ready, "waiting": waiting}
}

// startBreak pauses the room for min minutes (cmd op "break").
func (r *Room) startBreak(m *Member, in Inbound) {
	if r.media == nil {
		r.deny(m, in.Cid, "no_media")
		return
	}
	mins := in.Min
	if mins <= 0 {
		mins = breakDefaultMin
	}
	mins = min(mins, breakMaxMin)
	now := r.nowMs()
	if r.mode != ModePaused {
		r.cancelTimer()
		r.waiting = nil
		r.anchorPos = r.position(now)
		r.mode = ModePaused
		r.anchorTs = now
		r.startAt = 0
	}
	r.brk = &breakState{by: m.Name, min: mins, until: now + int64(mins)*60_000, ready: map[string]bool{}}
	r.publish("pause", m, in.Cid)
	r.broadcast(enc(r.breakMsg()))
	r.activity("pause", m.ID, fmt.Sprintf("%s %d dk mola verdi", m.Name, mins))
}

// breakReady: m is back from the break (cmd op "ready").
func (r *Room) breakReady(m *Member, in Inbound) {
	if r.brk == nil {
		r.deny(m, in.Cid, "no_break")
		return
	}
	if r.brk.ready[m.ID] {
		return
	}
	r.brk.ready[m.ID] = true
	r.checkBreak()
}

// checkBreak ends the break when nobody connected is still away; the room starts
// with the "Hazır mısınız?" 3-2-1. Otherwise everyone gets the new counts.
func (r *Room) checkBreak() {
	if r.brk == nil {
		return
	}
	ready, waiting := r.breakWaiters()
	if len(waiting) > 0 || len(ready) == 0 {
		r.broadcast(enc(r.breakMsg()))
		return
	}
	r.endBreak("ready")
	r.readyLead = true
	r.startCountdown(r.anchorPos, "release", nil, "")
	r.activity("release", "", "Herkes hazır, devam ediliyor")
}

// endBreak drops the break without starting playback (play, seek, new media...).
func (r *Room) endBreak(cause string) {
	if r.brk == nil {
		return
	}
	r.brk = nil
	r.broadcast(enc(map[string]any{"t": "break", "end": true, "cause": cause}))
}

// ---- history ----------------------------------------------------------

func sameMedia(a, b *Media) bool {
	return a != nil && b != nil && a.Kind == b.Kind && a.Src == b.Src && a.File == b.File
}

// noteLeftAt stores where the room is in the current media before it changes.
func (r *Room) noteLeftAt() {
	if len(r.history) > 0 && r.history[0].Media == r.media && r.media != nil {
		r.history[0].Pos = r.position(r.nowMs())
	}
}

// addHistory puts md on top of the room history (an older line of it goes away).
func (r *Room) addHistory(md *Media, by *Member) {
	r.history = slices.DeleteFunc(r.history, func(x histItem) bool { return sameMedia(x.Media, md) })
	r.history = slices.Insert(r.history, 0, histItem{Media: md, By: by.Name, At: r.nowMs()})
	if len(r.history) > historySize {
		r.history = r.history[:historySize]
	}
	r.broadcastHistory()
}

func (r *Room) historyOut() []histItem {
	out := slices.Clone(r.history)
	if len(out) > 0 && out[0].Media == r.media && r.media != nil {
		out[0].Pos = r.position(r.nowMs())
	}
	return out
}

func (r *Room) broadcastHistory() {
	items := r.historyOut()
	if items == nil {
		items = []histItem{}
	}
	r.broadcast(enc(map[string]any{"t": "history", "items": items}))
}
