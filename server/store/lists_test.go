package store

import (
	"database/sql"
	"path/filepath"
	"testing"

	_ "modernc.org/sqlite"
)

// A p0.24 database (list_items without stars/word) opens and keeps its rows.
func TestMigrateStars(t *testing.T) {
	path := filepath.Join(t.TempDir(), "koni.db")
	db, err := sql.Open("sqlite", "file:"+path)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := db.Exec(`CREATE TABLE list_items (user_id INTEGER NOT NULL, list TEXT NOT NULL, ref TEXT NOT NULL,
	kind TEXT NOT NULL DEFAULT 'movie', title TEXT NOT NULL, year INTEGER NOT NULL DEFAULT 0, poster TEXT NOT NULL DEFAULT '',
	at INTEGER NOT NULL, rating INTEGER NOT NULL DEFAULT 0, mates TEXT NOT NULL DEFAULT '[]', source TEXT NOT NULL DEFAULT '',
	PRIMARY KEY (user_id, list, ref));
	INSERT INTO list_items(user_id, list, ref, title, at, rating) VALUES(1, 'seen', 'tt1', 'Eski', 1, 1);`); err != nil {
		t.Fatal(err)
	}
	db.Close()
	s, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	if err := s.RateStars(1, "tt1", 4, "iyi"); err != nil {
		t.Fatal(err)
	}
	es, err := s.List(1, ListSeen)
	if err != nil || len(es) != 1 || es[0].Rating != 1 || es[0].Stars != 4 || es[0].Word != "iyi" {
		t.Fatalf("migrated: %v %v", es, err)
	}
}
