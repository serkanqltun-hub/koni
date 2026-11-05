package store

import (
	"database/sql"
	"errors"
	"math"
	"slices"
	"strconv"
	"strings"
)

// p0.27: series tracking (episodes), what a team watched together (team_progress),
// shared intro marks (skips) and friends' favourites for recommendations.

// Series is the stored air schedule of one series ref, refreshed once a day.
type Series struct {
	Ref     string
	Title   string
	Poster  string
	Season  int    // next episode; 0 = none announced
	Episode int    //
	Name    string // next episode's name
	Airs    string // next episode's air date, YYYY-MM-DD
	Aired   int    // episodes aired so far (specials not counted)
	Checked int64  // unix ms
}

// SeriesInfo returns the stored schedules of refs (missing ones are left out).
func (s *Store) SeriesInfo(refs []string) (map[string]Series, error) {
	out := map[string]Series{}
	for _, ref := range refs {
		var x Series
		err := s.db.QueryRow(`SELECT ref, title, poster, season, episode, name, airs, aired, checked FROM episodes WHERE ref = ?`, ref).
			Scan(&x.Ref, &x.Title, &x.Poster, &x.Season, &x.Episode, &x.Name, &x.Airs, &x.Aired, &x.Checked)
		if err == nil {
			out[ref] = x
		} else if !errors.Is(err, sql.ErrNoRows) {
			return nil, err
		}
	}
	return out, nil
}

// PutSeries stores x and stamps it checked now.
func (s *Store) PutSeries(x Series) error {
	_, err := s.db.Exec(`INSERT INTO episodes(ref, title, poster, season, episode, name, airs, aired, checked) VALUES(?,?,?,?,?,?,?,?,?)
ON CONFLICT(ref) DO UPDATE SET title = excluded.title, poster = excluded.poster, season = excluded.season, episode = excluded.episode,
  name = excluded.name, airs = excluded.airs, aired = excluded.aired, checked = excluded.checked`,
		x.Ref, x.Title, x.Poster, x.Season, x.Episode, x.Name, x.Airs, x.Aired, s.now().UnixMilli())
	return err
}

// teamKey is ",3,7,9,": the sorted account ids of a team.
func teamKey(ids []int64) string {
	ids = slices.Clone(ids)
	slices.Sort(ids)
	ids = slices.Compact(ids)
	var b strings.Builder
	b.WriteByte(',')
	for _, id := range ids {
		b.WriteString(strconv.FormatInt(id, 10))
		b.WriteByte(',')
	}
	return b.String()
}

// SetTeamProgress records that accounts ids watched episode season:episode of series
// ref together in room. The team's row only moves forward.
func (s *Store) SetTeamProgress(ids []int64, ref, title, room string, season, episode int) error {
	if len(ids) < 2 || ref == "" || episode < 1 {
		return ErrBadEntry
	}
	_, err := s.db.Exec(`INSERT INTO team_progress(team, ref, title, room, season, episode, at) VALUES(?,?,?,?,?,?,?)
ON CONFLICT(team, ref) DO UPDATE SET title = excluded.title, room = excluded.room, season = excluded.season,
  episode = excluded.episode, at = excluded.at
WHERE excluded.season > team_progress.season OR (excluded.season = team_progress.season AND excluded.episode >= team_progress.episode)`,
		teamKey(ids), ref, title, room, season, episode, s.now().UnixMilli())
	return err
}

// Team is a series one team (you and Members) watches together.
type Team struct {
	Ref     string   `json:"ref"`
	Title   string   `json:"title,omitempty"`
	Room    string   `json:"room,omitempty"`
	Season  int      `json:"season"`
	Episode int      `json:"episode"`
	Members []Holder `json:"members"`         // the others in the team
	Ahead   []string `json:"ahead,omitempty"` // members who marked a later episode seen
	At      int64    `json:"at"`
}

// TeamProgress lists the teams user is in, newest first (at most 20).
func (s *Store) TeamProgress(user int64) ([]Team, error) {
	rows, err := s.db.Query(`SELECT team, ref, title, room, season, episode, at FROM team_progress
WHERE team LIKE ? ORDER BY at DESC LIMIT 20`, "%,"+strconv.FormatInt(user, 10)+",%")
	if err != nil {
		return nil, err
	}
	type row struct {
		Team
		key string
	}
	var rs []row
	for rows.Next() {
		var r row
		if err := rows.Scan(&r.key, &r.Ref, &r.Title, &r.Room, &r.Season, &r.Episode, &r.At); err != nil {
			rows.Close()
			return nil, err
		}
		rs = append(rs, r)
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return nil, err
	}
	out := []Team{}
	for _, r := range rs {
		t := r.Team
		t.Members = []Holder{}
		for _, f := range strings.Split(strings.Trim(r.key, ","), ",") {
			id, _ := strconv.ParseInt(f, 10, 64)
			if id == user || id == 0 {
				continue
			}
			u, err := s.UserByID(id)
			if err != nil {
				continue // account gone
			}
			t.Members = append(t.Members, Holder{Name: u.Handle(), Avatar: u.Avatar})
			ahead, err := s.seenPast(id, t.Ref, t.Season, t.Episode)
			if err != nil {
				return nil, err
			}
			if ahead {
				t.Ahead = append(t.Ahead, u.Handle())
			}
		}
		out = append(out, t)
	}
	return out, nil
}

// seenPast: user marked an episode of ref after season:episode as seen.
func (s *Store) seenPast(user int64, ref string, season, episode int) (bool, error) {
	rows, err := s.db.Query(`SELECT ep FROM seen_eps WHERE user_id = ? AND ref = ?`, user, ref)
	if err != nil {
		return false, err
	}
	defer rows.Close()
	for rows.Next() {
		var ep string
		if err := rows.Scan(&ep); err != nil {
			return false, err
		}
		a, b, _ := strings.Cut(ep, ":")
		se, _ := strconv.Atoi(a)
		e, _ := strconv.Atoi(b)
		if se > season || (se == season && e > episode) {
			return true, nil
		}
	}
	return false, rows.Err()
}

// Skip is a shared intro / recap mark of a series (season 0 = every season).
type Skip struct {
	Season     int     `json:"season"`
	IntroStart float64 `json:"introStart"`
	IntroEnd   float64 `json:"introEnd"`
	RecapEnd   float64 `json:"recapEnd,omitempty"`
	By         string  `json:"by,omitempty"` // Ad#1234 who set it last
	At         int64   `json:"at"`
}

// maxSkipSec: marks must fall in an episode's first hour.
const maxSkipSec = 3600

func badSec(x float64) bool { return math.IsNaN(x) || math.IsInf(x, 0) || x < 0 || x > maxSkipSec }

// SetSkip writes ref's mark for season; the last writer wins. introEnd 0 means no
// intro (then recapEnd is required).
func (s *Store) SetSkip(user int64, ref string, k Skip) error {
	ref = strings.TrimSpace(ref)
	if ref == "" || len(ref) > 64 || k.Season < 0 || k.Season > 9999 ||
		badSec(k.IntroStart) || badSec(k.IntroEnd) || badSec(k.RecapEnd) ||
		(k.IntroEnd > 0 && k.IntroStart >= k.IntroEnd) || (k.IntroEnd == 0 && (k.IntroStart != 0 || k.RecapEnd == 0)) {
		return ErrBadEntry
	}
	_, err := s.db.Exec(`INSERT INTO skips(ref, season, intro_start, intro_end, recap_end, by_user, at) VALUES(?,?,?,?,?,?,?)
ON CONFLICT(ref, season) DO UPDATE SET intro_start = excluded.intro_start, intro_end = excluded.intro_end,
  recap_end = excluded.recap_end, by_user = excluded.by_user, at = excluded.at`,
		ref, k.Season, k.IntroStart, k.IntroEnd, k.RecapEnd, user, s.now().UnixMilli())
	return err
}

// ClearSkip removes ref's mark for season.
func (s *Store) ClearSkip(ref string, season int) error {
	_, err := s.db.Exec(`DELETE FROM skips WHERE ref = ? AND season = ?`, ref, season)
	return err
}

// Skips returns every mark of ref, season 0 first.
func (s *Store) Skips(ref string) ([]Skip, error) {
	rows, err := s.db.Query(`SELECT k.season, k.intro_start, k.intro_end, k.recap_end, COALESCE(u.username || '#' || u.tag, ''), k.at
FROM skips k LEFT JOIN users u ON u.id = k.by_user WHERE k.ref = ? ORDER BY k.season`, ref)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []Skip{}
	for rows.Next() {
		var k Skip
		if err := rows.Scan(&k.Season, &k.IntroStart, &k.IntroEnd, &k.RecapEnd, &k.By, &k.At); err != nil {
			return nil, err
		}
		out = append(out, k)
	}
	return out, rows.Err()
}

// Liked is a title the viewer hasn't seen that mutual friends rated 4+ stars.
type Liked struct {
	Entry
	By []Holder `json:"by"`
}

// FriendsLiked: titles mutual friends rated 4–5 stars that viewer hasn't seen; the
// most liked first, then the newest (at most n).
func (s *Store) FriendsLiked(viewer int64, n int) ([]Liked, error) {
	rows, err := s.db.Query(`
SELECT l.ref, l.kind, l.title, l.year, l.poster, l.at, u.username || '#' || u.tag, u.avatar, l.stars, l.word FROM list_items l
JOIN users u ON u.id = l.user_id
JOIN friends a ON a.user_id = ? AND a.friend_id = l.user_id
JOIN friends b ON b.user_id = l.user_id AND b.friend_id = a.user_id
WHERE l.list = 'seen' AND l.stars >= 4
  AND l.ref NOT IN (SELECT ref FROM list_items WHERE user_id = ? AND list = 'seen')
ORDER BY l.at DESC LIMIT 300`, viewer, viewer)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []Liked{}
	at := map[string]int{}
	for rows.Next() {
		var e Entry
		var h Holder
		if err := rows.Scan(&e.Ref, &e.Kind, &e.Title, &e.Year, &e.Poster, &e.At, &h.Name, &h.Avatar, &h.Stars, &h.Word); err != nil {
			return nil, err
		}
		if i, ok := at[e.Ref]; ok {
			out[i].By = append(out[i].By, h)
			continue
		}
		at[e.Ref] = len(out)
		out = append(out, Liked{Entry: e, By: []Holder{h}})
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	slices.SortStableFunc(out, func(a, b Liked) int { return len(b.By) - len(a.By) })
	return out[:min(len(out), n)], nil
}
