package hub

// Film nights (p0.28, PROTOCOL.md §3.0p): one-shot `event` (create / rsvp / cancel /
// list) and `presence.events`. At the planned time the server opens a room named
// after the night; the guests get a call, the host sees the room on the card.

import (
	"time"

	"github.com/serkanqltun-hub/koni/server/store"
)

const (
	planAhead   = 60 * 24 * time.Hour // how far ahead a night may be planned
	planShown   = 6 * time.Hour       // a night stays on the cards this long after it starts
	planLate    = 2 * time.Hour       // a night missed by more than this (server down) is not opened
	planRefresh = 15 * time.Second    // due nights are looked for at most this often
)

// eventRoom is the room a night opened, while it is still open.
type eventRoom struct {
	Invite string `json:"invite"`
	Name   string `json:"name"`
}

type eventOut struct {
	store.Plan
	Room *eventRoom `json:"room,omitempty"`
}

// Event answers an `event` handshake.
func (h *Hub) Event(in Inbound) map[string]any {
	u, e := h.account(in.Auth)
	if e != nil {
		return e
	}
	st := h.cfg.Store
	now := h.cfg.Now()
	out := map[string]any{"t": "event"}
	var err error
	switch in.Op {
	case "", "list":
	case "create":
		if in.At <= now.UnixMilli() || in.At > now.Add(planAhead).UnixMilli() {
			return errMsg("bad_entry")
		}
		var guests []int64
		for _, g := range in.Guests {
			x, err := st.Lookup(g)
			if err != nil {
				return errMsg(errCode(err))
			}
			guests = append(guests, x.ID)
		}
		var id int64
		if id, err = st.CreatePlan(u.ID, in.Title, in.At, guests); err == nil {
			out["id"] = id
		}
	case "rsvp":
		err = st.RSVP(in.ID, u.ID, in.Rsvp)
	case "cancel":
		err = st.CancelPlan(in.ID, u.ID)
	default:
		return errMsg("bad_message")
	}
	if err != nil {
		return errMsg(errCode(err))
	}
	if in.Op != "" && in.Op != "list" {
		h.soc.start.Do(func() { go h.presenceLoop() }) // opens the room on time
		h.pokePresence()
	}
	evs, err := h.eventsFor(u.ID)
	if err != nil {
		return errMsg("server_error")
	}
	out["events"] = evs
	return out
}

// eventsFor: the nights user hosts or is invited to (presence.events and `event`).
func (h *Hub) eventsFor(user int64) ([]eventOut, error) {
	plans, err := h.cfg.Store.Plans(user, h.cfg.Now().Add(-planShown).UnixMilli())
	if err != nil {
		return nil, err
	}
	out := make([]eventOut, 0, len(plans))
	for _, p := range plans {
		p.Mine = p.HostID == user
		o := eventOut{Plan: p}
		if p.Invite != "" {
			h.mu.Lock()
			r := h.byToken[p.Invite]
			h.mu.Unlock()
			if r != nil && !r.closedNow() {
				o.Room = &eventRoom{Invite: p.Invite, Name: r.name}
			}
		}
		out = append(out, o)
	}
	return out, nil
}

// openDuePlans opens the rooms of the nights whose time has come (presence loop).
func (h *Hub) openDuePlans() {
	st := h.cfg.Store
	if st == nil {
		return
	}
	now := h.cfg.Now()
	h.soc.mu.Lock()
	if now.Sub(h.soc.plansAt) < planRefresh {
		h.soc.mu.Unlock()
		return
	}
	h.soc.plansAt = now
	h.soc.mu.Unlock()
	due, err := st.DuePlans(now.UnixMilli())
	if err != nil {
		return
	}
	for _, p := range due {
		if now.UnixMilli()-p.At > planLate.Milliseconds() {
			_ = st.MarkPlan(p.ID, store.PlanMissed, "")
			continue
		}
		h.openPlan(p)
	}
}

// openPlan opens an empty room for p on its host's behalf: the host and every guest
// who did not say no may enter it; the guests get a call on their home screen.
func (h *Hub) openPlan(p store.Plan) {
	st := h.cfg.Store
	host, err := st.UserByID(p.HostID)
	if err != nil {
		_ = st.MarkPlan(p.ID, store.PlanMissed, "")
		return
	}
	id, inv := "r-"+token(6), token(12)
	r := newRoom(id, p.Title, inv, PolicyFree, h.cfg, h.remove)
	r.plan = p.ID
	r.guests = map[int64]bool{p.HostID: true}
	for _, g := range p.Guests {
		if g.Rsvp != "no" {
			r.guests[g.ID] = true
		}
	}
	if err := st.MarkPlan(p.ID, store.PlanOpen, inv); err != nil {
		r.do(r.shutdown)
		return
	}
	h.mu.Lock()
	h.byID[id] = r
	h.byToken[inv] = r
	h.mu.Unlock()
	r.do(func() {
		r.recordUser("room_create", p.HostID, map[string]any{"name": r.name, "policy": r.policy, "event": p.ID})
		r.idle() // nobody is inside yet: it closes after EmptyWait unless someone comes
	})
	now := h.cfg.Now().UnixMilli()
	h.soc.mu.Lock()
	for g := range r.guests {
		if g == p.HostID {
			continue
		}
		h.soc.calls[g] = append(h.soc.calls[g], roomCall{room: r, From: host.Handle(), Avatar: host.Avatar, Name: r.name, Invite: inv, At: now})
	}
	h.soc.mu.Unlock()
	h.pokePresence()
}
