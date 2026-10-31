package hub

import (
	"fmt"
	"sort"
	"time"
)

// Shared play queue (Lounge, docs/design/media.md §6.3, docs/PROTOCOL.md §3.11/§4.16).
// Order: most "next" votes first, then oldest. When the room's media ends (any member
// reports track_end for it) the head becomes the room media and starts after
// Config.QueueStart, which gives every client time to resolve and load it.
const (
	maxQueue        = 50
	maxQueuePerUser = 10
)

type queueItem struct {
	id     string
	media  *Media
	by     string // member id
	byName string
	n      int // add order
	voters map[string]bool
}

type queueState struct {
	items []*queueItem
	next  int
	atEnd *Media // room media that already reported its end (nothing was queued then)
	// playing: the room media came from the queue (music): its time counts as listening.
	playing *Media
	// playingBy: who queued `playing`; failed: members whose player could not open it.
	playingBy string
	failed    map[string]bool
}

func (r *Room) queueMsg() []byte {
	items := make([]map[string]any, 0, len(r.queue.items))
	for _, it := range r.queue.items {
		voters := make([]string, 0, len(it.voters))
		for id := range it.voters {
			voters = append(voters, id)
		}
		sort.Strings(voters)
		items = append(items, map[string]any{"id": it.id, "media": it.media, "by": it.by, "byName": it.byName, "voters": voters})
	}
	return enc(map[string]any{"t": "queue", "items": items})
}

func (r *Room) sortQueue() {
	sort.SliceStable(r.queue.items, func(i, j int) bool {
		a, b := r.queue.items[i], r.queue.items[j]
		if len(a.voters) != len(b.voters) {
			return len(a.voters) > len(b.voters)
		}
		return a.n < b.n
	})
}

func (r *Room) handleQueueAdd(m *Member, in Inbound) {
	md, ok := cleanMedia(in.Media)
	if !ok || md.Kind != "url" || md.Referer != "" {
		r.funError(m, "Kuyruğa yalnızca web adresi eklenir")
		return
	}
	mine := 0
	for _, it := range r.queue.items {
		if it.media.Src == md.Src {
			r.funError(m, "Bu zaten kuyrukta")
			return
		}
		if it.by == m.ID {
			mine++
		}
	}
	switch {
	case len(r.queue.items) >= maxQueue:
		r.funError(m, "Kuyruk dolu")
		return
	case mine >= maxQueuePerUser:
		r.funError(m, fmt.Sprintf("Kuyrukta en fazla %d şarkın olabilir", maxQueuePerUser))
		return
	}
	r.queue.next++
	r.queue.items = append(r.queue.items, &queueItem{
		id: fmt.Sprintf("q%d", r.queue.next), media: md, by: m.ID, byName: m.Name,
		n: r.queue.next, voters: map[string]bool{},
	})
	r.sortQueue()
	r.activity("queue_add", m.ID, fmt.Sprintf("%s kuyruğa ekledi: %s", m.Name, md.Title))
	if r.queueIdle() {
		r.advanceQueue(nil) // nothing to wait for: start right away
		return
	}
	r.broadcast(r.queueMsg())
}

func (r *Room) findQueued(id string) int {
	for i, it := range r.queue.items {
		if it.id == id {
			return i
		}
	}
	return -1
}

// handleQueueVote toggles the member's "play this next" vote.
func (r *Room) handleQueueVote(m *Member, in Inbound) {
	i := r.findQueued(in.QueueID)
	if i < 0 {
		return
	}
	it := r.queue.items[i]
	if it.voters[m.ID] {
		delete(it.voters, m.ID)
	} else {
		it.voters[m.ID] = true
	}
	r.sortQueue()
	r.broadcast(r.queueMsg())
}

func (r *Room) handleQueueRemove(m *Member, in Inbound) {
	i := r.findQueued(in.QueueID)
	if i < 0 {
		return
	}
	if r.queue.items[i].by != m.ID && !m.canControl() {
		r.funError(m, "Başkasının şarkısını yalnızca host çıkarabilir")
		return
	}
	r.queue.items = append(r.queue.items[:i], r.queue.items[i+1:]...)
	r.broadcast(r.queueMsg())
}

// queueIdle: an added song may start at once. Nothing is loaded, the room media
// already ended, or the room media is not a queue song and is not playing (e.g. a
// paused film left over from Sinema).
func (r *Room) queueIdle() bool {
	return r.media == nil || r.queue.atEnd == r.media || (r.media != r.queue.playing && r.mode == ModePaused)
}

// handleQueueNext skips to the head of the queue now. in.Src is the song the member
// saw playing: when two members press "next" together (or one double-clicks), the
// second press would skip the song that just started, so it is ignored.
func (r *Room) handleQueueNext(m *Member, in Inbound) {
	switch {
	case !r.mayControl(m):
		r.funError(m, "Bu odada şarkıyı yalnızca host geçebilir")
	case in.Src != "" && (r.media == nil || in.Src != r.media.Src):
		// already skipped
	case len(r.queue.items) == 0:
		r.funError(m, "Kuyruk boş")
	default:
		r.advanceQueue(m)
	}
}

// handleTrackEnd: a member's player reached the end of src. The first report for the
// current media advances the queue; later ones (other members) no longer match. With
// nothing queued the room pauses at the end instead of "playing" silence.
// failed: the member's player could not open src (dead link, yt-dlp error). It is
// skipped when the member who queued it, or more than half of the room, cannot open
// it; otherwise the room goes on without that member until the next song. Kind "debrid"
// is only skipped by the member who opened it from the queue: the rest just detach.
func (r *Room) handleTrackEnd(m *Member, in Inbound) {
	if r.media == nil || in.Src != r.media.Src || r.queue.atEnd == r.media {
		return
	}
	if in.Failed {
		if !r.trackFailed(m) {
			return
		}
		r.activity("skip", m.ID, "Açılamadı, geçildi: "+r.media.Title)
	} else if m.detached {
		return // plays on its own clock: its end is not the room's end
	}
	r.queue.atEnd = r.media
	if len(r.queue.items) > 0 {
		r.advanceQueue(nil)
		return
	}
	r.cancelTimer()
	r.waiting = nil
	r.anchorPos = r.position(r.nowMs())
	if d := r.media.Duration; d > 0 && r.anchorPos > d {
		r.anchorPos = d
	}
	r.anchorTs, r.startAt = r.nowMs(), 0
	r.mode = ModePaused
	r.publish("pause", nil, "")
	if r.media == r.queue.playing {
		r.feed("queue_end", "", "Sıra bitti; şarkı ekleyince çalmaya devam eder")
	}
}

// trackFailed records that m cannot open the room media; true: skip it for everyone.
func (r *Room) trackFailed(m *Member) bool {
	if r.queue.failed == nil {
		r.queue.failed = map[string]bool{}
	}
	r.queue.failed[m.ID] = true
	if r.media == r.queue.playing && r.queue.playingBy == m.ID {
		return true
	}
	n, bad := 0, 0
	for _, x := range r.members {
		if x.ghost || x.closed {
			continue
		}
		n++
		if r.queue.failed[x.ID] {
			bad++
		}
	}
	// Personal-source media (kind "debrid"): each member plays it with their own provider,
	// so members without one never take the film away from those who have it.
	if bad*2 > n && r.media.Kind != "debrid" {
		return true
	}
	if !m.detached {
		r.detach(m, nil)
	}
	return false
}

// advanceQueue makes the head the room media and plays it after queueStartDelay.
func (r *Room) advanceQueue(by *Member) {
	it := r.queue.items[0]
	r.queue.items = r.queue.items[1:]
	// The same song again: players still hold it (at its end) and do not reload it, so
	// the start is a "seek" to 0 (a plain start never moves a player backwards).
	again := r.media != nil && r.media.Src == it.media.Src
	r.switchMedia(it.media, by, "")
	r.queue.playing = it.media
	r.queue.playingBy = it.by
	r.queue.failed = nil
	r.broadcast(r.queueMsg())
	md := r.media
	cause := "play"
	if again {
		cause = "seek"
	}
	startDelay := r.cfg.QueueStart
	if len(r.members) <= 1 {
		startDelay = 50 * time.Millisecond
	} else if startDelay > 750*time.Millisecond {
		startDelay = 750 * time.Millisecond
	}
	time.AfterFunc(startDelay, func() {
		r.do(func() {
			if r.media == md && r.mode == ModePaused && len(r.members) > 0 {
				r.started = true // the QueueStart wait already was the "get ready" time
				r.startCountdown(0, cause, nil, "")
			}
		})
	})
}
