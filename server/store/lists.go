package store

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"math"
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
	stars   INTEGER NOT NULL DEFAULT 0,
	word    TEXT NOT NULL DEFAULT '',
	PRIMARY KEY (user_id, list, ref)
);
CREATE INDEX IF NOT EXISTS list_items_ref ON list_items(ref);
CREATE TABLE IF NOT EXISTS progress (
	user_id  INTEGER NOT NULL REFERENCES users(id),
	ref      TEXT NOT NULL,
	kind     TEXT NOT NULL DEFAULT 'movie',
	season   INTEGER NOT NULL DEFAULT 0,
	episode  INTEGER NOT NULL DEFAULT 0,
	pos      REAL NOT NULL,
	duration REAL NOT NULL DEFAULT 0,
	title    TEXT NOT NULL DEFAULT '',
	poster   TEXT NOT NULL DEFAULT '',
	at       INTEGER NOT NULL,
	PRIMARY KEY (user_id, ref)
);
CREATE TABLE IF NOT EXISTS seen_eps (
	user_id INTEGER NOT NULL REFERENCES users(id),
	ref     TEXT NOT NULL,
	ep      TEXT NOT NULL,
	at      INTEGER NOT NULL,
	PRIMARY KEY (user_id, ref, ep)
);
CREATE TABLE IF NOT EXISTS episodes (
	ref     TEXT PRIMARY KEY,
	title   TEXT NOT NULL DEFAULT '',
	poster  TEXT NOT NULL DEFAULT '',
	season  INTEGER NOT NULL DEFAULT 0,
	episode INTEGER NOT NULL DEFAULT 0,
	name    TEXT NOT NULL DEFAULT '',
	airs    TEXT NOT NULL DEFAULT '',
	aired   INTEGER NOT NULL DEFAULT 0,
	checked INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS team_progress (
	team    TEXT NOT NULL,
	ref     TEXT NOT NULL,
	title   TEXT NOT NULL DEFAULT '',
	room    TEXT NOT NULL DEFAULT '',
	season  INTEGER NOT NULL,
	episode INTEGER NOT NULL,
	at      INTEGER NOT NULL,
	PRIMARY KEY (team, ref)
);
CREATE TABLE IF NOT EXISTS skips (
	ref         TEXT NOT NULL,
	season      INTEGER NOT NULL DEFAULT 0,
	intro_start REAL NOT NULL DEFAULT 0,
	intro_end   REAL NOT NULL DEFAULT 0,
	recap_end   REAL NOT NULL DEFAULT 0,
	by_user     INTEGER NOT NULL,
	at          INTEGER NOT NULL,
	PRIMARY KEY (ref, season)
);
`

// migrateStars adds the p0.25 star rating columns to a pre-p0.25 list_items table.
func migrateStars(db *sql.DB) error {
	var n int
	if err := db.QueryRow(`SELECT count(*) FROM pragma_table_info('list_items') WHERE name = 'stars'`).Scan(&n); err != nil || n > 0 {
		return err
	}
	for _, q := range []string{
		`ALTER TABLE list_items ADD COLUMN stars INTEGER NOT NULL DEFAULT 0`,
		`ALTER TABLE list_items ADD COLUMN word TEXT NOT NULL DEFAULT ''`,
	} {
		if _, err := db.Exec(q); err != nil {
			return err
		}
	}
	return nil
}

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
	// p0.25 (seen): 1–5 stars and one short word; Episodes = watched episodes "s:e"
	Stars    int      `json:"stars,omitempty"`
	Word     string   `json:"word,omitempty"`
	Episodes []string `json:"episodes,omitempty"`
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
	rows, err := s.db.Query(`SELECT ref, kind, title, year, poster, at, rating, mates, source, stars, word FROM list_items
WHERE user_id = ? AND list = ? ORDER BY at DESC LIMIT 300`, user, list)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []Entry{}
	for rows.Next() {
		var e Entry
		var mates string
		if err := rows.Scan(&e.Ref, &e.Kind, &e.Title, &e.Year, &e.Poster, &e.At, &e.Rating, &mates, &e.Source, &e.Stars, &e.Word); err != nil {
			return nil, err
		}
		_ = json.Unmarshal([]byte(mates), &e.With)
		out = append(out, e)
	}
	if err := rows.Err(); err != nil || list != ListSeen {
		return out, err
	}
	eps, err := s.Episodes(user)
	if err != nil {
		return nil, err
	}
	for i := range out {
		out[i].Episodes = eps[out[i].Ref]
	}
	return out, nil
}

// Holder is a mutual friend who has a title on a list.
type Holder struct {
	Name   string `json:"name"` // Ad#1234
	Avatar string `json:"avatar,omitempty"`
	Rating int    `json:"rating,omitempty"`
	Stars  int    `json:"stars,omitempty"` // seen (p0.25)
	Word   string `json:"word,omitempty"`
}

// Who lists the viewer's mutual friends that want / have seen ref (künye).
func (s *Store) Who(viewer int64, ref string) (want, seen []Holder, err error) {
	rows, err := s.db.Query(`
SELECT l.list, u.username || '#' || u.tag, u.avatar, l.rating, l.stars, l.word FROM list_items l
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
		if err := rows.Scan(&list, &h.Name, &h.Avatar, &h.Rating, &h.Stars, &h.Word); err != nil {
			return nil, nil, err
		}
		if list == ListSeen {
			seen = append(seen, h)
		} else {
			h.Rating, h.Stars, h.Word = 0, 0, ""
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

// MaxWord is the longest word a star rating may carry (runes).
const MaxWord = 24

// RateStars sets 1–5 stars and an optional short word on a seen title (p0.25);
// stars below 1 clear both. The ❤️/👎 rating is left as it is.
func (s *Store) RateStars(user int64, ref string, stars int, word string) error {
	word = strings.TrimSpace(word)
	if stars < 1 {
		stars, word = 0, ""
	}
	stars = min(5, stars)
	if r := []rune(word); len(r) > MaxWord {
		word = string(r[:MaxWord])
	}
	_, err := s.db.Exec(`UPDATE list_items SET stars = ?, word = ? WHERE user_id = ? AND list = ? AND ref = ?`, stars, word, user, ListSeen, ref)
	return err
}

// ProgressKeep: how many "continue watching" rows a user keeps (newest win).
const ProgressKeep = 50

// Progress is where a user stopped in a title (p0.25, "Kaldığın yer"). A series has
// one row: the episode watched last.
type Progress struct {
	Ref      string  `json:"ref"`
	Kind     string  `json:"kind"` // "movie" | "series"
	Season   int     `json:"season,omitempty"`
	Episode  int     `json:"episode,omitempty"`
	Pos      float64 `json:"pos"`                // seconds
	Duration float64 `json:"duration,omitempty"` // seconds
	Title    string  `json:"title,omitempty"`
	Poster   string  `json:"poster,omitempty"`
	At       int64   `json:"at"`
}

// SetProgress writes p for user and keeps the newest ProgressKeep rows.
func (s *Store) SetProgress(user int64, p Progress) error {
	p.Ref, p.Title = strings.TrimSpace(p.Ref), strings.TrimSpace(p.Title)
	if p.Kind != "series" {
		p.Kind, p.Season, p.Episode = "movie", 0, 0
	}
	if p.Ref == "" || len(p.Ref) > 64 || len([]rune(p.Title)) > 200 || len(p.Poster) > 500 ||
		math.IsNaN(p.Pos) || math.IsInf(p.Pos, 0) || p.Pos < 0 || p.Pos > 1e6 ||
		math.IsNaN(p.Duration) || math.IsInf(p.Duration, 0) || p.Duration < 0 || p.Duration > 1e6 ||
		p.Season < 0 || p.Season > 9999 || p.Episode < 0 || p.Episode > 99999 {
		return ErrBadEntry
	}
	if !strings.HasPrefix(p.Poster, "https://") {
		p.Poster = ""
	}
	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if _, err := tx.Exec(`INSERT INTO progress(user_id, ref, kind, season, episode, pos, duration, title, poster, at)
VALUES(?,?,?,?,?,?,?,?,?,?)
ON CONFLICT(user_id, ref) DO UPDATE SET kind = excluded.kind, season = excluded.season, episode = excluded.episode,
  pos = excluded.pos, duration = excluded.duration, at = excluded.at,
  title = CASE WHEN excluded.title != '' THEN excluded.title ELSE title END,
  poster = CASE WHEN excluded.poster != '' THEN excluded.poster ELSE poster END`,
		user, p.Ref, p.Kind, p.Season, p.Episode, p.Pos, p.Duration, p.Title, p.Poster, s.now().UnixMilli()); err != nil {
		return err
	}
	if _, err := tx.Exec(`DELETE FROM progress WHERE user_id = ?1 AND ref NOT IN
  (SELECT ref FROM progress WHERE user_id = ?1 ORDER BY at DESC LIMIT ?2)`, user, ProgressKeep); err != nil {
		return err
	}
	return tx.Commit()
}

// RemoveProgress drops ref from user's "continue watching".
func (s *Store) RemoveProgress(user int64, ref string) error {
	_, err := s.db.Exec(`DELETE FROM progress WHERE user_id = ? AND ref = ?`, user, ref)
	return err
}

// ProgressList is user's "continue watching", newest first.
func (s *Store) ProgressList(user int64) ([]Progress, error) {
	rows, err := s.db.Query(`SELECT ref, kind, season, episode, pos, duration, title, poster, at FROM progress
WHERE user_id = ? ORDER BY at DESC LIMIT ?`, user, ProgressKeep)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []Progress{}
	for rows.Next() {
		var p Progress
		if err := rows.Scan(&p.Ref, &p.Kind, &p.Season, &p.Episode, &p.Pos, &p.Duration, &p.Title, &p.Poster, &p.At); err != nil {
			return nil, err
		}
		out = append(out, p)
	}
	return out, rows.Err()
}

// SeenEpisode marks (on) or unmarks one episode of series ref as watched.
func (s *Store) SeenEpisode(user int64, ref string, season, episode int, on bool) error {
	ref = strings.TrimSpace(ref)
	if ref == "" || len(ref) > 64 || season < 0 || season > 9999 || episode < 1 || episode > 99999 {
		return ErrBadEntry
	}
	ep := fmt.Sprintf("%d:%d", season, episode)
	var err error
	if on {
		_, err = s.db.Exec(`INSERT INTO seen_eps(user_id, ref, ep, at) VALUES(?,?,?,?) ON CONFLICT DO NOTHING`, user, ref, ep, s.now().UnixMilli())
	} else {
		_, err = s.db.Exec(`DELETE FROM seen_eps WHERE user_id = ? AND ref = ? AND ep = ?`, user, ref, ep)
	}
	return err
}

// Episodes: user's watched episodes per series ref, "s:e" in watch order.
func (s *Store) Episodes(user int64) (map[string][]string, error) {
	rows, err := s.db.Query(`SELECT ref, ep FROM seen_eps WHERE user_id = ? ORDER BY at, ep`, user)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := map[string][]string{}
	for rows.Next() {
		var ref, ep string
		if err := rows.Scan(&ref, &ep); err != nil {
			return nil, err
		}
		out[ref] = append(out[ref], ep)
	}
	return out, rows.Err()
}
