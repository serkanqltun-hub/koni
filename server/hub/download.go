package hub

import (
	"fmt"
	"math"
	"strings"
)

// "Herkes İndirsin" (docs/design/media.md §6.1): media kind "download" opens a task;
// every member downloads the page with their own yt-dlp and reports dl_progress. When
// every connected, non-detached member is ready (or failed, as long as someone is
// ready) the room starts by itself. Any playback command before that ("start with
// whoever is ready") ends the barrier; late members join in when their file is done.

const (
	DlQueued      = "queued"
	DlDownloading = "downloading"
	DlReady       = "ready"
	DlFailed      = "failed"

	maxDlError = 200
)

type dlTask struct {
	id      string
	started bool // playback started (barrier or by hand): never auto-start again
	status  map[string]*DlStatus
}

func (r *Room) newDownload() string {
	id := fmt.Sprintf("d%d", r.seq+1)
	r.dl = &dlTask{id: id, status: map[string]*DlStatus{}}
	return id
}

// dlReady: m has the task's file (or there is no task).
func (r *Room) dlReady(m *Member) bool {
	if r.dl == nil {
		return true
	}
	s := r.dl.status[m.ID]
	return s != nil && s.State == DlReady
}

func (r *Room) dlProgress(m *Member, in Inbound) {
	d := r.dl
	if d == nil || in.TaskID != d.id {
		return // stale task: the room moved on
	}
	switch in.State {
	case DlQueued, DlDownloading, DlReady, DlFailed:
	default:
		return
	}
	st := &DlStatus{User: m.ID, Name: m.Name, State: in.State, Pct: clampPct(in.Pct), Speed: nonNeg(in.Speed), Eta: nonNeg(in.Eta)}
	if st.State == DlReady {
		st.Pct = 100
	}
	if st.State == DlFailed {
		e := strings.TrimSpace(in.Error)
		if rs := []rune(e); len(rs) > maxDlError {
			e = string(rs[:maxDlError])
		}
		st.Error = e
	}
	prev := d.status[m.ID]
	d.status[m.ID] = st
	r.broadcast(enc(struct {
		T      string `json:"t"`
		TaskID string `json:"taskId"`
		*DlStatus
	}{"dl_status", d.id, st}))
	if prev == nil || prev.State != st.State {
		switch st.State {
		case DlReady:
			r.activity("download", m.ID, m.Name+" indirmeyi bitirdi")
		case DlFailed:
			r.activity("download", m.ID, fmt.Sprintf("%s indiremedi: %s", m.Name, st.Error))
		}
	}
	r.checkBarrier()
}

// checkBarrier starts the room once everyone who counts has the file.
func (r *Room) checkBarrier() {
	d := r.dl
	if d == nil || d.started || r.mode != ModePaused {
		return
	}
	ready := 0
	for _, m := range r.members {
		if m.ghost || m.detached {
			continue
		}
		s := d.status[m.ID]
		if s == nil {
			return
		}
		switch s.State {
		case DlReady:
			ready++
		case DlFailed:
		default:
			return
		}
	}
	if ready == 0 {
		return
	}
	d.started = true
	r.waiting = nil
	r.startCountdown(r.anchorPos, "download_ready", nil, "")
	r.activity("play", "", "Herkes indirdi, başlıyor")
}

func (r *Room) downloads() []DlStatus {
	if r.dl == nil {
		return nil
	}
	var out []DlStatus
	for _, id := range r.order {
		if s := r.dl.status[id]; s != nil {
			out = append(out, *s)
		}
	}
	return out
}

func clampPct(p float64) float64 {
	if math.IsNaN(p) || p < 0 {
		return 0
	}
	return math.Min(p, 100)
}

func nonNeg(v float64) float64 {
	if math.IsNaN(v) || math.IsInf(v, 0) || v < 0 {
		return 0
	}
	return v
}
