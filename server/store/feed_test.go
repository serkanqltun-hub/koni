package store

import (
	"path/filepath"
	"testing"
	"time"
)

func TestFeedGroupsWatchersAndJoins(t *testing.T) {
	s, err := Open(filepath.Join(t.TempDir(), "koni.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	root, _ := s.CreateInvite(0, time.Hour)
	serkan, _, err := s.Register(root, "Serkan", "uzun-sifre", "", "")
	if err != nil {
		t.Fatal(err)
	}
	code, _ := s.CreateInvite(serkan.ID, time.Hour)
	ayse, _, err := s.Register(code, "Ayşe", "uzun-sifre", "", "") // mutual with Serkan via the code
	if err != nil {
		t.Fatal(err)
	}
	root2, _ := s.CreateInvite(0, time.Hour)
	cem, _, _ := s.Register(root2, "Cem", "uzun-sifre", "", "") // not a friend
	now := time.Now().UnixMilli()
	for _, q := range []struct {
		room string
		user int64
		typ  string
		body string
	}{
		{"r1", serkan.ID, "room_create", `{"name":"Cuma"}`},
		{"r1", serkan.ID, "watch", `{"ms":1200000,"title":"Prisoners"}`},
		{"r1", ayse.ID, "watch", `{"ms":2400000,"title":"Prisoners"}`},
		{"r1", cem.ID, "watch", `{"ms":3600000,"title":"Prisoners"}`}, // stranger: hidden
		{"r2", ayse.ID, "watch", `{"ms":60000,"title":"Kısa"}`},       // under 5 min
	} {
		if _, err := s.db.Exec(`INSERT INTO events(ts, room_id, user_id, type, payload) VALUES(?,?,?,?,?)`, now, q.room, q.user, q.typ, q.body); err != nil {
			t.Fatal(err)
		}
	}
	items, err := s.Feed(serkan.ID, now-int64(time.Hour/time.Millisecond), 10)
	if err != nil {
		t.Fatal(err)
	}
	var watch, joined *FeedItem
	for i := range items {
		switch items[i].Kind {
		case "watch":
			watch = &items[i]
		case "joined":
			joined = &items[i]
		}
	}
	if len(items) != 2 || watch == nil || joined == nil {
		t.Fatalf("items: %+v", items)
	}
	if watch.Title != "Prisoners" || watch.Room != "Cuma" || len(watch.Users) != 2 || watch.Users[0] != ayse.Handle() || watch.Sec != 2400 {
		t.Fatalf("watch line: %+v", watch)
	}
	if joined.Users[0] != ayse.Handle() || joined.By != serkan.Handle() {
		t.Fatalf("joined line: %+v", joined)
	}
}
