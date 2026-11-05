package store

import (
	"database/sql"
	"math"
	"sort"
	"time"
)

// Turkey has no DST since 2016: one fixed zone for "night" and period starts.
var trt = time.FixedZone("TRT", 3*3600)

// NightHours: [0, NightEnd) local hour counts as night watching (Gece Kuşu).
const NightEnd = 4

// MarathonSec: one presence of at least this much watching earns Maratoncu.
const MarathonSec = 4 * 3600

// UserStats is one account's totals for a period (docs/design/accounts.md §5.4).
type UserStats struct {
	User       string `json:"user"` // "Ad#1234"
	Avatar     string `json:"avatar"`
	Days       int64  `json:"days"` // distinct days with watching
	WatchSec   int64  `json:"watchSec"`
	NightSec   int64  `json:"nightSec"`
	LongestSec int64  `json:"longestSec"` // longest single presence in a room
	Pauses     int64  `json:"pauses"`
	Seeks      int64  `json:"seeks"`     // seek + skip
	Holds      int64  `json:"holds"`     // times the room waited for this member
	Picked     int64  `json:"picked"`    // own wheel proposals that won
	Rejected   int64  `json:"rejected"`  // own wheel proposals that lost
	Likes      int64  `json:"likes"`     // swipe likes
	Swipes     int64  `json:"swipes"`    // all swipe votes
	Queued     int64  `json:"queued"`    // songs added to the Lounge queue
	ListenSec  int64  `json:"listenSec"` // Lounge (queue) listening
}

// EasyMinSwipes: Kolay Beğenen needs this many votes, so one lucky like doesn't win.
const EasyMinSwipes = 10

// Badge is awarded to everyone tied at the top (or past the threshold).
type Badge struct {
	ID    string   `json:"id"`
	Title string   `json:"title"`
	Rule  string   `json:"rule"`
	Users []string `json:"users"`
}

// PeriodStart is the unix ms where "all", "month" or "week" (Monday) begins, Turkey time.
func PeriodStart(period string, now time.Time) (int64, bool) {
	t := now.In(trt)
	switch period {
	case "all":
		return 0, true
	case "month":
		return time.Date(t.Year(), t.Month(), 1, 0, 0, 0, 0, trt).UnixMilli(), true
	case "week":
		d := (int(t.Weekday()) + 6) % 7 // Monday = 0
		return time.Date(t.Year(), t.Month(), t.Day()-d, 0, 0, 0, 0, trt).UnixMilli(), true
	}
	return 0, false
}

// NightMs is how much of [from, to) (unix ms) falls between 00:00 and NightEnd, Turkey time.
func NightMs(from, to int64) int64 {
	var n int64
	t := time.UnixMilli(from).In(trt)
	day := time.Date(t.Year(), t.Month(), t.Day(), 0, 0, 0, 0, trt)
	for ; day.UnixMilli() < to; day = day.AddDate(0, 0, 1) {
		a, b := max(from, day.UnixMilli()), min(to, day.Add(NightEnd*time.Hour).UnixMilli())
		if b > a {
			n += b - a
		}
	}
	return n
}

// Stats returns totals since `since` (unix ms) for viewer and viewer's mutual friends:
// metrics are visible only to people who added each other (friends.go).
func (s *Store) Stats(viewer, since int64) ([]UserStats, error) {
	return s.StatsRange(viewer, since, math.MaxInt64)
}

// StatsRange is Stats for [since, until) (unix ms).
func (s *Store) StatsRange(viewer, since, until int64) ([]UserStats, error) {
	rows, err := s.db.Query(`
WITH mates AS (
  SELECT a.friend_id AS user_id FROM friends a
  JOIN friends b ON b.user_id = a.friend_id AND b.friend_id = a.user_id
  WHERE a.user_id = ?
  UNION SELECT ?
)
SELECT u.username || '#' || u.tag, u.avatar,
  COUNT(DISTINCT CASE WHEN e.type = 'watch' THEN (e.ts + 10800000) / 86400000 END),
  COALESCE(SUM(CASE WHEN e.type = 'watch' THEN json_extract(e.payload, '$.ms') END), 0) / 1000,
  COALESCE(SUM(CASE WHEN e.type = 'watch' THEN json_extract(e.payload, '$.night') END), 0) / 1000,
  COALESCE(MAX(CASE WHEN e.type = 'watch' THEN json_extract(e.payload, '$.session') END), 0) / 1000,
  COUNT(CASE WHEN e.type = 'pause' THEN 1 END),
  COUNT(CASE WHEN e.type IN ('seek', 'skip') THEN 1 END),
  COUNT(CASE WHEN e.type = 'hold' THEN 1 END),
  COUNT(CASE WHEN e.type = 'proposal' AND json_extract(e.payload, '$.won') = 1 THEN 1 END),
  COUNT(CASE WHEN e.type = 'proposal' AND json_extract(e.payload, '$.won') = 0 THEN 1 END),
  COUNT(CASE WHEN e.type = 'vote' AND json_extract(e.payload, '$.like') = 1 THEN 1 END),
  COUNT(CASE WHEN e.type = 'vote' THEN 1 END),
  COUNT(CASE WHEN e.type = 'queue_add' THEN 1 END),
  COALESCE(SUM(CASE WHEN e.type = 'listen' THEN json_extract(e.payload, '$.ms') END), 0) / 1000
FROM mates m
JOIN users u ON u.id = m.user_id
LEFT JOIN events e ON e.user_id = m.user_id AND e.ts >= ? AND e.ts < ?
GROUP BY u.id
ORDER BY u.username COLLATE NOCASE`, viewer, viewer, since, until)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []UserStats
	for rows.Next() {
		var u UserStats
		if err := rows.Scan(&u.User, &u.Avatar, &u.Days, &u.WatchSec, &u.NightSec, &u.LongestSec, &u.Pauses, &u.Seeks, &u.Holds,
			&u.Picked, &u.Rejected, &u.Likes, &u.Swipes, &u.Queued, &u.ListenSec); err != nil {
			return nil, err
		}
		out = append(out, u)
	}
	return out, rows.Err()
}

// Badges applies the §5.4 rules that the recorded events support. Leaders need a
// value above zero; ties share the badge.
func Badges(stats []UserStats) []Badge {
	leader := func(id, title, rule string, v func(UserStats) int64) Badge {
		b := Badge{ID: id, Title: title, Rule: rule}
		var best int64
		for _, u := range stats {
			switch x := v(u); {
			case x > best:
				best, b.Users = x, []string{u.User}
			case x == best && x > 0:
				b.Users = append(b.Users, u.User)
			}
		}
		return b
	}
	out := []Badge{
		leader("pause", "🍿 Cips Molası Şampiyonu", "En çok durduran", func(u UserStats) int64 { return u.Pauses }),
		leader("seek", "⏩ Seek Manyağı", "En çok ileri/geri saran", func(u UserStats) int64 { return u.Seeks }),
		leader("night", "🦉 Gece Kuşu", "00:00–04:00 arası en çok izleyen", func(u UserStats) int64 { return u.NightSec }),
		leader("stall", "📡 Modem Sesi", "Odayı en çok bekleten", func(u UserStats) int64 { return u.Holds }),
		leader("taste", "🎯 Zevk Diktatörü", "Çarkta önerisi en çok seçilen", func(u UserStats) int64 { return u.Picked }),
		leader("rejected", "🗑️ Reddedilen Dahi", "Çarkta önerisi en çok elenen", func(u UserStats) int64 { return u.Rejected }),
		leader("easy", "❤️ Kolay Beğenen", "Swipe'ta en yüksek beğeni oranı (en az 10 oy)", func(u UserStats) int64 {
			if u.Swipes < EasyMinSwipes {
				return 0
			}
			return u.Likes * 1000 / u.Swipes
		}),
		leader("dj", "🎧 DJ Koltuğu", "Kuyruğa en çok şarkı ekleyen", func(u UserStats) int64 { return u.Queued }),
	}
	marathon := Badge{ID: "marathon", Title: "🏃 Maratoncu", Rule: "Tek oturumda 4+ saat izleyen"}
	for _, u := range stats {
		if u.LongestSec >= MarathonSec {
			marathon.Users = append(marathon.Users, u.User)
		}
	}
	out = append(out, marathon)
	for i := range out {
		sort.Strings(out[i].Users)
	}
	return out
}

// Daily is user's watch seconds for each of the last 7 days (Turkey time), oldest first.
func (s *Store) Daily(user int64, now time.Time) ([7]int64, error) {
	var out [7]int64
	t := now.In(trt)
	today := time.Date(t.Year(), t.Month(), t.Day(), 0, 0, 0, 0, trt)
	from := today.AddDate(0, 0, -6).UnixMilli()
	rows, err := s.db.Query(`SELECT ts, json_extract(payload, '$.ms') FROM events WHERE user_id = ? AND type = 'watch' AND ts >= ?`, user, from)
	if err != nil {
		return out, err
	}
	defer rows.Close()
	for rows.Next() {
		var ts int64
		var ms sql.NullInt64
		if err := rows.Scan(&ts, &ms); err != nil {
			return out, err
		}
		if d := int((ts - from) / 86400000); d >= 0 && d < 7 {
			out[d] += ms.Int64 / 1000
		}
	}
	return out, rows.Err()
}

// MonthRange is the unix ms range of month "YYYY-MM", Turkey time; "" is the
// current month. The label is the month in "YYYY-MM" form.
func MonthRange(month string, now time.Time) (from, to int64, label string, ok bool) {
	t := now.In(trt)
	start := time.Date(t.Year(), t.Month(), 1, 0, 0, 0, 0, trt)
	if month != "" {
		p, err := time.ParseInLocation("2006-01", month, trt)
		if err != nil || p.Year() < 2020 || p.After(t) {
			return 0, 0, "", false
		}
		start = p
	}
	return start.UnixMilli(), start.AddDate(0, 1, 0).UnixMilli(), start.Format("2006-01"), true
}

// Mate is a mutual friend you watched with in a recap month.
type Mate struct {
	Name   string `json:"name"`
	Avatar string `json:"avatar,omitempty"`
	Sec    int64  `json:"sec"`   // time watched together (the shorter of the two, per room)
	Rooms  int64  `json:"rooms"` // rooms you were both in
}

// TeamPick is the title with the highest team score of a recap month.
type TeamPick struct {
	Ref    string  `json:"ref"`
	Title  string  `json:"title"`
	Poster string  `json:"poster,omitempty"`
	Stars  float64 `json:"stars"` // average of you and your mutual friends, 1–5
	Votes  int     `json:"votes"`
}

// RecapData is one account's month (p0.28 `recap`); genres are added by the hub.
type RecapData struct {
	WatchSec int64     `json:"watchSec"`
	Days     int64     `json:"days"`
	Hours    [24]int64 `json:"hours"`    // watch seconds by hour of day, Turkey time
	PeakHour int       `json:"peakHour"` // -1 when nothing was watched
	Movies   int       `json:"movies"`   // titles that went onto İzlediklerim this month
	Series   int       `json:"series"`
	Mates    []Mate    `json:"mates"`
	Best     *TeamPick `json:"best,omitempty"`
	Seen     []Entry   `json:"-"` // for the genres
}

// Recap gathers user's month [from, to).
func (s *Store) Recap(user, from, to int64) (RecapData, error) {
	out := RecapData{PeakHour: -1, Mates: []Mate{}}
	rows, err := s.db.Query(`SELECT ts, json_extract(payload, '$.ms') FROM events WHERE user_id = ? AND type = 'watch' AND ts >= ? AND ts < ?`, user, from, to)
	if err != nil {
		return out, err
	}
	days := map[int64]bool{}
	for rows.Next() {
		var ts int64
		var ms sql.NullInt64
		if err := rows.Scan(&ts, &ms); err != nil {
			rows.Close()
			return out, err
		}
		sec := ms.Int64 / 1000
		out.WatchSec += sec
		out.Hours[time.UnixMilli(ts).In(trt).Hour()] += sec
		days[(ts+10800000)/86400000] = true
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return out, err
	}
	out.Days = int64(len(days))
	for h, sec := range out.Hours {
		if sec > 0 && (out.PeakHour < 0 || sec > out.Hours[out.PeakHour]) {
			out.PeakHour = h
		}
	}

	mates, err := s.db.Query(`
WITH mine AS (
  SELECT room_id, SUM(json_extract(payload, '$.ms')) AS ms FROM events
  WHERE user_id = ? AND type = 'watch' AND ts >= ? AND ts < ? AND room_id NOT LIKE 'solo:%' GROUP BY room_id
), mates AS (
  SELECT a.friend_id AS user_id FROM friends a
  JOIN friends b ON b.user_id = a.friend_id AND b.friend_id = a.user_id WHERE a.user_id = ?
), theirs AS (
  SELECT e.user_id, e.room_id, SUM(json_extract(e.payload, '$.ms')) AS ms FROM events e JOIN mates m ON m.user_id = e.user_id
  WHERE e.type = 'watch' AND e.ts >= ? AND e.ts < ? AND e.room_id IN (SELECT room_id FROM mine) GROUP BY e.user_id, e.room_id
)
SELECT u.username || '#' || u.tag, u.avatar, SUM(MIN(t.ms, m.ms)) / 1000 AS sec, COUNT(*)
FROM theirs t JOIN mine m ON m.room_id = t.room_id JOIN users u ON u.id = t.user_id
GROUP BY t.user_id ORDER BY sec DESC LIMIT 3`, user, from, to, user, from, to)
	if err != nil {
		return out, err
	}
	for mates.Next() {
		var m Mate
		if err := mates.Scan(&m.Name, &m.Avatar, &m.Sec, &m.Rooms); err != nil {
			mates.Close()
			return out, err
		}
		out.Mates = append(out.Mates, m)
	}
	mates.Close()
	if err := mates.Err(); err != nil {
		return out, err
	}

	seen, err := s.db.Query(`SELECT ref, kind, title, poster FROM list_items WHERE user_id = ? AND list = ? AND at >= ? AND at < ? ORDER BY at DESC`, user, ListSeen, from, to)
	if err != nil {
		return out, err
	}
	for seen.Next() {
		var e Entry
		if err := seen.Scan(&e.Ref, &e.Kind, &e.Title, &e.Poster); err != nil {
			seen.Close()
			return out, err
		}
		if e.Kind == "series" {
			out.Series++
		} else if e.Kind == "movie" {
			out.Movies++
		}
		out.Seen = append(out.Seen, e)
	}
	seen.Close()
	if err := seen.Err(); err != nil {
		return out, err
	}

	// "En yüksek ekip puanı": titles you watched with someone this month, scored by
	// you and your mutual friends.
	var b TeamPick
	err = s.db.QueryRow(`
WITH team AS (
  SELECT a.friend_id AS user_id FROM friends a
  JOIN friends c ON c.user_id = a.friend_id AND c.friend_id = a.user_id WHERE a.user_id = ?
  UNION SELECT ?
)
SELECT l.ref, l.title, l.poster, AVG(o.stars), COUNT(o.stars) FROM list_items l
JOIN list_items o ON o.ref = l.ref AND o.list = 'seen' AND o.stars > 0 AND o.user_id IN (SELECT user_id FROM team)
WHERE l.user_id = ? AND l.list = 'seen' AND l.at >= ? AND l.at < ? AND l.mates != '[]'
GROUP BY l.ref ORDER BY 4 DESC, 5 DESC, l.at DESC LIMIT 1`, user, user, user, from, to).Scan(&b.Ref, &b.Title, &b.Poster, &b.Stars, &b.Votes)
	switch {
	case err == sql.ErrNoRows:
	case err != nil:
		return out, err
	default:
		b.Stars = math.Round(b.Stars*10) / 10
		out.Best = &b
	}
	return out, nil
}
