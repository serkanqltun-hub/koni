package store

import (
	"cmp"
	"slices"
)

// FeedItem is one line of the home screen's "Etkinlik" tab (p0.22): what you and your
// mutual friends watched together, and who joined KONİ.
type FeedItem struct {
	Kind  string   `json:"kind"`            // "watch" | "joined"
	At    int64    `json:"at"`              // unix ms, newest event of the line
	Users []string `json:"users"`           // Ad#1234; most time first
	Title string   `json:"title,omitempty"` // watch: what was on
	Room  string   `json:"room,omitempty"`  // watch: room name
	Sec   int64    `json:"sec,omitempty"`   // watch: the longest single watcher's time
	By    string   `json:"by,omitempty"`    // joined: whose invite code
}

// Feed lists the viewer's and their mutual friends' activity since `since`, newest
// first, at most `limit` lines. A watch line groups everyone who watched the same
// title in the same room; lines under five minutes are dropped.
func (s *Store) Feed(viewer, since int64, limit int) ([]FeedItem, error) {
	rows, err := s.db.Query(`
WITH mates AS (
  SELECT a.friend_id AS user_id FROM friends a
  JOIN friends b ON b.user_id = a.friend_id AND b.friend_id = a.user_id
  WHERE a.user_id = ?
  UNION SELECT ?
)
SELECT e.room_id, COALESCE(json_extract(e.payload, '$.title'), ''), u.username || '#' || u.tag,
  SUM(json_extract(e.payload, '$.ms')) / 1000, MAX(e.ts),
  COALESCE((SELECT json_extract(c.payload, '$.name') FROM events c WHERE c.room_id = e.room_id AND c.type = 'room_create' LIMIT 1), '')
FROM events e JOIN mates m ON m.user_id = e.user_id JOIN users u ON u.id = e.user_id
WHERE e.type = 'watch' AND e.ts >= ?
GROUP BY e.room_id, 2, e.user_id`, viewer, viewer, since)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	type key struct{ room, title string }
	type watcher struct {
		name string
		sec  int64
	}
	groups := map[key]*FeedItem{}
	who := map[key][]watcher{}
	for rows.Next() {
		var room, title, name, roomName string
		var sec, at int64
		if err := rows.Scan(&room, &title, &name, &sec, &at, &roomName); err != nil {
			return nil, err
		}
		k := key{room, title}
		g := groups[k]
		if g == nil {
			g = &FeedItem{Kind: "watch", Title: title, Room: roomName}
			groups[k] = g
		}
		g.At = max(g.At, at)
		g.Sec = max(g.Sec, sec)
		who[k] = append(who[k], watcher{name, sec})
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	var out []FeedItem
	for k, g := range groups {
		if g.Sec < 300 {
			continue
		}
		ws := who[k]
		slices.SortFunc(ws, func(a, b watcher) int { return cmp.Compare(b.sec, a.sec) })
		for _, w := range ws {
			g.Users = append(g.Users, w.name)
		}
		out = append(out, *g)
	}

	joined, err := s.db.Query(`
SELECT u.username || '#' || u.tag, u.created_at, COALESCE(b.username || '#' || b.tag, '')
FROM users u
JOIN friends a ON a.user_id = ? AND a.friend_id = u.id
JOIN friends r ON r.user_id = u.id AND r.friend_id = a.user_id
LEFT JOIN invites i ON i.used_by = u.id
LEFT JOIN users b ON b.id = i.created_by
WHERE u.created_at >= ?`, viewer, since)
	if err != nil {
		return nil, err
	}
	defer joined.Close()
	for joined.Next() {
		var it FeedItem
		var name string
		if err := joined.Scan(&name, &it.At, &it.By); err != nil {
			return nil, err
		}
		it.Kind, it.Users = "joined", []string{name}
		out = append(out, it)
	}
	if err := joined.Err(); err != nil {
		return nil, err
	}
	slices.SortFunc(out, func(a, b FeedItem) int { return cmp.Compare(b.At, a.At) })
	if len(out) > limit {
		out = out[:limit]
	}
	return out, nil
}
