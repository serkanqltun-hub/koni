package store

import (
	"path/filepath"
	"testing"
	"time"
)

func TestMonthRange(t *testing.T) {
	now := time.Date(2026, 11, 5, 23, 0, 0, 0, trt)
	from, to, label, ok := MonthRange("", now)
	if !ok || label != "2026-11" || from != time.Date(2026, 11, 1, 0, 0, 0, 0, trt).UnixMilli() || to != time.Date(2026, 12, 1, 0, 0, 0, 0, trt).UnixMilli() {
		t.Fatalf("this month: %d %d %s %v", from, to, label, ok)
	}
	if _, _, label, ok := MonthRange("2026-10", now); !ok || label != "2026-10" {
		t.Fatal("last month")
	}
	for _, m := range []string{"2026-13", "ekim", "2026-12", "2019-01"} {
		if _, _, _, ok := MonthRange(m, now); ok {
			t.Fatalf("%q accepted", m)
		}
	}
}

func TestRecap(t *testing.T) {
	s, err := Open(filepath.Join(t.TempDir(), "koni.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	root, _ := s.CreateInvite(0, time.Hour)
	serkan, _, _ := s.Register(root, "Serkan", "uzun-sifre", "", "")
	code, _ := s.CreateInvite(serkan.ID, time.Hour)
	ayse, _, _ := s.Register(code, "Ayşe", "uzun-sifre", "", "")
	from, to, _, _ := MonthRange("2026-10", time.Date(2026, 11, 5, 12, 0, 0, 0, trt))
	at := func(day, hour int) int64 { return time.Date(2026, 10, day, hour, 0, 0, 0, trt).UnixMilli() }
	for _, q := range []struct {
		ts   int64
		room string
		user int64
		ms   int
	}{
		{at(3, 22), "r1", serkan.ID, 3600000},
		{at(3, 22), "r1", ayse.ID, 1800000},
		{at(4, 21), "solo:1", serkan.ID, 600000},
		{at(4, 21), "solo:2", ayse.ID, 600000},                                          // solo: not together
		{time.Date(2026, 9, 30, 22, 0, 0, 0, trt).UnixMilli(), "r1", serkan.ID, 999000}, // last month
	} {
		if _, err := s.db.Exec(`INSERT INTO events(ts, room_id, user_id, type, payload) VALUES(?,?,?,'watch',json_object('ms', ?))`, q.ts, q.room, q.user, q.ms); err != nil {
			t.Fatal(err)
		}
	}
	for _, l := range []struct {
		user       int64
		ref, mates string
		stars      int
	}{
		{serkan.ID, "tt1", `["Ayşe"]`, 4},
		{ayse.ID, "tt1", `["Serkan"]`, 5},
		{serkan.ID, "tt2", `[]`, 5}, // alone: not a team pick
	} {
		if _, err := s.db.Exec(`INSERT INTO list_items(user_id, list, ref, kind, title, at, mates, stars) VALUES(?,'seen',?,'movie',?,?,?,?)`,
			l.user, l.ref, "Film "+l.ref, at(5, 20), l.mates, l.stars); err != nil {
			t.Fatal(err)
		}
	}
	r, err := s.Recap(serkan.ID, from, to)
	if err != nil {
		t.Fatal(err)
	}
	if r.WatchSec != 4200 || r.Days != 2 || r.PeakHour != 22 || r.Hours[21] != 600 || r.Movies != 2 {
		t.Fatalf("totals: %+v", r)
	}
	if len(r.Mates) != 1 || r.Mates[0].Name != ayse.Handle() || r.Mates[0].Sec != 1800 || r.Mates[0].Rooms != 1 {
		t.Fatalf("mates: %+v", r.Mates)
	}
	if r.Best == nil || r.Best.Ref != "tt1" || r.Best.Stars != 4.5 || r.Best.Votes != 2 {
		t.Fatalf("best: %+v", r.Best)
	}
	if empty, _ := s.Recap(ayse.ID, to, to+1); empty.PeakHour != -1 || empty.Best != nil || len(empty.Mates) != 0 {
		t.Fatalf("empty month: %+v", empty)
	}
}

func TestPlans(t *testing.T) {
	s, err := Open(filepath.Join(t.TempDir(), "koni.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	root, _ := s.CreateInvite(0, time.Hour)
	host, _, _ := s.Register(root, "Serkan", "uzun-sifre", "", "")
	code, _ := s.CreateInvite(host.ID, time.Hour)
	guest, _, _ := s.Register(code, "Ayşe", "uzun-sifre", "", "")
	root2, _ := s.CreateInvite(0, time.Hour)
	stranger, _, _ := s.Register(root2, "Cem", "uzun-sifre", "", "")
	if _, err := s.CreatePlan(host.ID, "", 5000, []int64{stranger.ID}); err != ErrNotFriends {
		t.Fatalf("stranger: %v", err)
	}
	id, err := s.CreatePlan(host.ID, "  ", 5000, []int64{guest.ID, guest.ID, host.ID})
	if err != nil {
		t.Fatal(err)
	}
	if err := s.RSVP(id, stranger.ID, "yes"); err != ErrBadEntry {
		t.Fatalf("not a guest: %v", err)
	}
	if err := s.RSVP(id, guest.ID, "yes"); err != nil {
		t.Fatal(err)
	}
	ps, err := s.Plans(guest.ID, 0)
	if err != nil || len(ps) != 1 || ps[0].Title != "Film gecesi" || ps[0].Mine || len(ps[0].Guests) != 1 || ps[0].Guests[0].Rsvp != "yes" {
		t.Fatalf("guest's plans: %+v %v", ps, err)
	}
	if ps, _ := s.Plans(stranger.ID, 0); len(ps) != 0 {
		t.Fatalf("stranger sees %+v", ps)
	}
	if due, _ := s.DuePlans(4999); len(due) != 0 {
		t.Fatal("not due yet")
	}
	if due, _ := s.DuePlans(5000); len(due) != 1 {
		t.Fatal("due")
	}
	if err := s.CancelPlan(id, guest.ID); err != ErrBadEntry {
		t.Fatalf("guest cancels: %v", err)
	}
	if err := s.CancelPlan(id, host.ID); err != nil {
		t.Fatal(err)
	}
	if ps, _ := s.Plans(host.ID, 0); len(ps) != 0 {
		t.Fatalf("cancelled still shown: %+v", ps)
	}
}
