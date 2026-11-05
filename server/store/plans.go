package store

import (
	"slices"
	"strings"
)

// Film nights (p0.28): a host plans a night for some mutual friends; at that time the
// server opens a room for them.

const (
	PlanMaxGuests = 7  // a room holds 8
	PlanMaxOpen   = 10 // planned nights per host

	ErrTooManyPlans Error = "too_many_events"
	ErrNotFriends   Error = "not_friends"
)

const plansSchema = `
CREATE TABLE IF NOT EXISTS plans (
	id         INTEGER PRIMARY KEY,
	host_id    INTEGER NOT NULL REFERENCES users(id),
	title      TEXT NOT NULL,
	at         INTEGER NOT NULL,
	created_at INTEGER NOT NULL,
	state      TEXT NOT NULL DEFAULT '',
	invite     TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS plan_guests (
	plan_id INTEGER NOT NULL REFERENCES plans(id),
	user_id INTEGER NOT NULL REFERENCES users(id),
	rsvp    TEXT NOT NULL DEFAULT '',
	PRIMARY KEY (plan_id, user_id)
);
`

// Plan states: "" planned, "open" its room was opened, "cancel", "missed" (the
// server was down at the time).
const (
	PlanOpen   = "open"
	PlanCancel = "cancel"
	PlanMissed = "missed"
)

// Guest is one invited account and its answer: "yes" | "no" | "maybe" | "".
type Guest struct {
	Name   string `json:"name"`
	Avatar string `json:"avatar,omitempty"`
	Rsvp   string `json:"rsvp"`
	ID     int64  `json:"-"`
}

// Plan is a film night.
type Plan struct {
	ID     int64   `json:"id"`
	Title  string  `json:"title"`
	At     int64   `json:"at"`
	Host   Holder  `json:"host"`
	Guests []Guest `json:"guests"`
	Mine   bool    `json:"mine,omitempty"` // the viewer is the host
	State  string  `json:"state,omitempty"`
	HostID int64   `json:"-"`
	Invite string  `json:"-"` // room invite once opened
}

// CreatePlan adds a film night; guests must be the host's mutual friends.
func (s *Store) CreatePlan(host int64, title string, at int64, guests []int64) (int64, error) {
	title = strings.TrimSpace(title)
	if title == "" {
		title = "Film gecesi"
	}
	guests = slices.Compact(slices.Sorted(slices.Values(guests)))
	guests = slices.DeleteFunc(guests, func(g int64) bool { return g == host })
	if len([]rune(title)) > 80 || len(guests) > PlanMaxGuests {
		return 0, ErrBadEntry
	}
	for _, g := range guests {
		if ok, err := s.Mutual(host, g); err != nil {
			return 0, err
		} else if !ok {
			return 0, ErrNotFriends
		}
	}
	var n int
	if err := s.db.QueryRow(`SELECT count(*) FROM plans WHERE host_id = ? AND state = ''`, host).Scan(&n); err != nil {
		return 0, err
	}
	if n >= PlanMaxOpen {
		return 0, ErrTooManyPlans
	}
	tx, err := s.db.Begin()
	if err != nil {
		return 0, err
	}
	defer tx.Rollback()
	res, err := tx.Exec(`INSERT INTO plans(host_id, title, at, created_at) VALUES(?,?,?,?)`, host, title, at, s.now().UnixMilli())
	if err != nil {
		return 0, err
	}
	id, _ := res.LastInsertId()
	for _, g := range guests {
		if _, err := tx.Exec(`INSERT INTO plan_guests(plan_id, user_id) VALUES(?,?)`, id, g); err != nil {
			return 0, err
		}
	}
	return id, tx.Commit()
}

// RSVP records a guest's answer to a planned night.
func (s *Store) RSVP(plan, user int64, rsvp string) error {
	if rsvp != "yes" && rsvp != "no" && rsvp != "maybe" {
		return ErrBadEntry
	}
	res, err := s.db.Exec(`UPDATE plan_guests SET rsvp = ? WHERE plan_id = ? AND user_id = ?
AND plan_id IN (SELECT id FROM plans WHERE state IN ('', 'open'))`, rsvp, plan, user)
	if err != nil {
		return err
	}
	if n, _ := res.RowsAffected(); n == 0 {
		return ErrBadEntry
	}
	return nil
}

// CancelPlan: the host calls a planned night off.
func (s *Store) CancelPlan(plan, host int64) error {
	res, err := s.db.Exec(`UPDATE plans SET state = ? WHERE id = ? AND host_id = ? AND state = ''`, PlanCancel, plan, host)
	if err != nil {
		return err
	}
	if n, _ := res.RowsAffected(); n == 0 {
		return ErrBadEntry
	}
	return nil
}

// MarkPlan sets a night's state (and its room invite).
func (s *Store) MarkPlan(plan int64, state, invite string) error {
	_, err := s.db.Exec(`UPDATE plans SET state = ?, invite = ? WHERE id = ?`, state, invite, plan)
	return err
}

// Plans lists the nights user hosts or is invited to that start at or after since
// (cancelled ones left out), soonest first, at most 20.
func (s *Store) Plans(user, since int64) ([]Plan, error) {
	return s.plans(`WHERE p.state != 'cancel' AND p.at >= ? AND (p.host_id = ? OR p.id IN (SELECT plan_id FROM plan_guests WHERE user_id = ?))
ORDER BY p.at LIMIT 20`, user, since, user, user)
}

// DuePlans: planned nights whose time has come.
func (s *Store) DuePlans(now int64) ([]Plan, error) {
	return s.plans(`WHERE p.state = '' AND p.at <= ? ORDER BY p.at`, 0, now)
}

func (s *Store) plans(where string, viewer int64, args ...any) ([]Plan, error) {
	rows, err := s.db.Query(`SELECT p.id, p.title, p.at, p.state, p.invite, p.host_id, u.username || '#' || u.tag, u.avatar
FROM plans p JOIN users u ON u.id = p.host_id `+where, args...)
	if err != nil {
		return nil, err
	}
	out := []Plan{}
	for rows.Next() {
		var p Plan
		if err := rows.Scan(&p.ID, &p.Title, &p.At, &p.State, &p.Invite, &p.HostID, &p.Host.Name, &p.Host.Avatar); err != nil {
			rows.Close()
			return nil, err
		}
		p.Mine = viewer != 0 && p.HostID == viewer
		out = append(out, p)
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return nil, err
	}
	for i := range out {
		if out[i].Guests, err = s.planGuests(out[i].ID); err != nil {
			return nil, err
		}
	}
	return out, nil
}

func (s *Store) planGuests(plan int64) ([]Guest, error) {
	rows, err := s.db.Query(`SELECT g.user_id, u.username || '#' || u.tag, u.avatar, g.rsvp FROM plan_guests g
JOIN users u ON u.id = g.user_id WHERE g.plan_id = ? ORDER BY u.username COLLATE NOCASE`, plan)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []Guest{}
	for rows.Next() {
		var g Guest
		if err := rows.Scan(&g.ID, &g.Name, &g.Avatar, &g.Rsvp); err != nil {
			return nil, err
		}
		out = append(out, g)
	}
	return out, rows.Err()
}
