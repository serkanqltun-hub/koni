package store

import (
	"database/sql"
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
LEFT JOIN events e ON e.user_id = m.user_id AND e.ts >= ?
GROUP BY u.id
ORDER BY u.username COLLATE NOCASE`, viewer, viewer, since)
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
