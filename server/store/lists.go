package store

import (
	"encoding/json"
	"strings"
)

// Lists (p0.23): "İzleyeceklerim" (want) and "İzlediklerim" (seen) on profiles.
// A title is keyed by Ref: an IMDb id ("tt0111161") when known, else "tmdb:<kind>:<id>".

const (
	ListWant = "want"
	ListSeen = "seen"

	ErrBadEntry Error = "bad_entry" // missing ref/title or unknown list
)

const listsSchema = `
CREATE TABLE IF NOT EXISTS list_items (
	user_id INTEGER NOT NULL REFERENCES users(id),
	list    TEXT NOT NULL,
	ref     TEXT NOT NULL,
	kind    TEXT NOT NULL DEFAULT 'movie',
	title   TEXT NOT NULL,
	year    INTEGER NOT NULL DEFAULT 0,
	poster  TEXT NOT NULL DEFAULT '',
	at      INTEGER NOT NULL,
	rating  INTEGER NOT NULL DEFAULT 0,
	mates   TEXT NOT NULL DEFAULT '[]',
	source  TEXT NOT NULL DEFAULT '',
	PRIMARY KEY (user_id, list, ref)
);
CREATE INDEX IF NOT EXISTS list_items_ref ON list_items(ref);
`

// Entry is one title on a list.
type Entry struct {
	Ref    string   `json:"ref"`
	Kind   string   `json:"kind"` // "movie" | "series"
	Title  string   `json:"title"`
	Year   int      `json:"year,omitempty"`
	Poster string   `json:"poster,omitempty"`
	At     int64    `json:"at,omitempty"`
	Rating int      `json:"rating,omitempty"` // seen: 1 ❤️, -1 👎, 0 none
	With   []string `json:"with,omitempty"`   // seen: who you watched with (names)
	Source string   `json:"source,omitempty"` // want: "night" | "titles" | ""
}

func (e *Entry) clean() error {
	e.Ref, e.Title = strings.TrimSpace(e.Ref), strings.TrimSpace(e.Title)
	if e.Kind != "series" {
		e.Kind = "movie"
	}
	if e.Ref == "" || len(e.Ref) > 64 || e.Title == "" || len([]rune(e.Title)) > 200 || len(e.Poster) > 500 {
		return ErrBadEntry
	}
	if !strings.HasPrefix(e.Poster, "https://") {
		e.Poster = ""
	}
	return nil
}

// AddToList puts e on user's list; marking it seen takes it off "want".
func (s *Store) AddToList(user int64, list string, e Entry) error {
	if list != ListWant && list != ListSeen {
		return ErrBadEntry
	}
	if err := e.clean(); err != nil {
		return err
	}
	with, _ := json.Marshal(e.With)
	if e.With == nil {
		with = []byte("[]")
	}
	now := s.now().UnixMilli()
	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if _, err := tx.Exec(`INSERT INTO list_items(user_id, list, ref, kind, title, year, poster, at, rating, mates, source)
VALUES(?,?,?,?,?,?,?,?,?,?,?)
ON CONFLICT(user_id, list, ref) DO UPDATE SET at = excluded.at,
  poster = CASE WHEN excluded.poster != '' THEN excluded.poster ELSE poster END,
  mates = CASE WHEN excluded.mates != '[]' THEN excluded.mates ELSE mates END`,
		user, list, e.Ref, e.Kind, e.Title, e.Year, e.Poster, now, e.Rating, string(with), e.Source); err != nil {
		return err
	}
	if list == ListSeen {
		if _, err := tx.Exec(`DELETE FROM list_items WHERE user_id = ? AND list = ? AND ref = ?`, user, ListWant, e.Ref); err != nil {
			return err
		}
	}
	return tx.Commit()
}

// RemoveFromList drops ref from user's list.
func (s *Store) RemoveFromList(user int64, list, ref string) error {
	_, err := s.db.Exec(`DELETE FROM list_items WHERE user_id = ? AND list = ? AND ref = ?`, user, list, ref)
	return err
}

// RateSeen sets ❤️ (1) / 👎 (-1) / none (0) on a seen title.
func (s *Store) RateSeen(user int64, ref string, rating int) error {
	rating = max(-1, min(1, rating))
	_, err := s.db.Exec(`UPDATE list_items SET rating = ? WHERE user_id = ? AND list = ? AND ref = ?`, rating, user, ListSeen, ref)
	return err
}

// List returns user's list, newest first (at most 300).
func (s *Store) List(user int64, list string) ([]Entry, error) {
	rows, err := s.db.Query(`SELECT ref, kind, title, year, poster, at, rating, mates, source FROM list_items
WHERE user_id = ? AND list = ? ORDER BY at DESC LIMIT 300`, user, list)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []Entry{}
	for rows.Next() {
		var e Entry
		var mates string
		if err := rows.Scan(&e.Ref, &e.Kind, &e.Title, &e.Year, &e.Poster, &e.At, &e.Rating, &mates, &e.Source); err != nil {
			return nil, err
		}
		_ = json.Unmarshal([]byte(mates), &e.With)
		out = append(out, e)
	}
	return out, rows.Err()
}

// Holder is a mutual friend who has a title on a list.
type Holder struct {
	Name   string `json:"name"` // Ad#1234
	Avatar string `json:"avatar,omitempty"`
	Rating int    `json:"rating,omitempty"`
}

// Who lists the viewer's mutual friends that want / have seen ref (künye).
func (s *Store) Who(viewer int64, ref string) (want, seen []Holder, err error) {
	rows, err := s.db.Query(`
SELECT l.list, u.username || '#' || u.tag, u.avatar, l.rating FROM list_items l
JOIN users u ON u.id = l.user_id
JOIN friends a ON a.user_id = ? AND a.friend_id = l.user_id
JOIN friends b ON b.user_id = l.user_id AND b.friend_id = a.user_id
WHERE l.ref = ? ORDER BY l.at DESC`, viewer, ref)
	if err != nil {
		return nil, nil, err
	}
	defer rows.Close()
	want, seen = []Holder{}, []Holder{}
	for rows.Next() {
		var list string
		var h Holder
		if err := rows.Scan(&list, &h.Name, &h.Avatar, &h.Rating); err != nil {
			return nil, nil, err
		}
		if list == ListSeen {
			seen = append(seen, h)
		} else {
			h.Rating = 0
			want = append(want, h)
		}
	}
	return want, seen, rows.Err()
}

// WantMates: for each of the viewer's wanted refs, the mutual friends that want it
// too ("ORTAK" badges and the common list).
func (s *Store) WantMates(viewer int64) (map[string][]Holder, error) {
	rows, err := s.db.Query(`
SELECT l.ref, u.username || '#' || u.tag, u.avatar FROM list_items l
JOIN list_items mine ON mine.user_id = ? AND mine.list = 'want' AND mine.ref = l.ref
JOIN users u ON u.id = l.user_id
JOIN friends a ON a.user_id = ? AND a.friend_id = l.user_id
JOIN friends b ON b.user_id = l.user_id AND b.friend_id = a.user_id
WHERE l.list = 'want'`, viewer, viewer)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := map[string][]Holder{}
	for rows.Next() {
		var ref string
		var h Holder
		if err := rows.Scan(&ref, &h.Name, &h.Avatar); err != nil {
			return nil, err
		}
		out[ref] = append(out[ref], h)
	}
	return out, rows.Err()
}
