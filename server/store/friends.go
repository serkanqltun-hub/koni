package store

const (
	ErrUnknownUser Error = "unknown_user" // no account with that name
	ErrSelfFriend  Error = "self_friend"  // adding yourself
)

// Friend is one entry of a friends list. Mutual: they added you back, so you see each
// other's statistics.
type Friend struct {
	Name   string `json:"name"` // "Ad#1234"
	Avatar string `json:"avatar"`
	Mutual bool   `json:"mutual"`
	Admin  bool   `json:"admin,omitempty"`
	ID     int64  `json:"-"`
}

// FriendList is what the `friends` reply carries: people you added, and people who
// added you but you haven't added back.
type FriendList struct {
	Friends  []Friend `json:"friends"`
	Requests []Friend `json:"requests"`
}

// userID resolves "Ad#1234" (or a bare name only one account has).
func (s *Store) userID(handle string) (int64, error) {
	u, _, err := s.lookup(handle)
	return u.ID, err
}

// AddFriend adds name to user's friends. Adding again is a no-op.
func (s *Store) AddFriend(user int64, name string) error {
	id, err := s.userID(name)
	if err != nil {
		return err
	}
	if id == user {
		return ErrSelfFriend
	}
	_, err = s.db.Exec(`INSERT OR IGNORE INTO friends(user_id, friend_id, created_at) VALUES(?,?,?)`,
		user, id, s.now().UnixMilli())
	return err
}

// RemoveFriend drops name from user's friends; their statistics stop being shared both ways.
func (s *Store) RemoveFriend(user int64, name string) error {
	id, err := s.userID(name)
	if err != nil {
		return err
	}
	_, err = s.db.Exec(`DELETE FROM friends WHERE user_id = ? AND friend_id = ?`, user, id)
	return err
}

// DeclineFriend drops name's request to user (name added user, user won't add back).
func (s *Store) DeclineFriend(user int64, name string) error {
	id, err := s.userID(name)
	if err != nil {
		return err
	}
	_, err = s.db.Exec(`DELETE FROM friends WHERE user_id = ? AND friend_id = ?`, id, user)
	return err
}

// Friends lists user's friends and pending requests, by name.
func (s *Store) Friends(user int64) (FriendList, error) {
	out := FriendList{Friends: []Friend{}, Requests: []Friend{}}
	rows, err := s.db.Query(`
SELECT u.id, u.username || '#' || u.tag, u.avatar, u.is_admin, EXISTS(SELECT 1 FROM friends b WHERE b.user_id = a.friend_id AND b.friend_id = a.user_id)
FROM friends a JOIN users u ON u.id = a.friend_id
WHERE a.user_id = ?
ORDER BY u.username COLLATE NOCASE`, user)
	if err != nil {
		return out, err
	}
	defer rows.Close()
	for rows.Next() {
		var f Friend
		if err := rows.Scan(&f.ID, &f.Name, &f.Avatar, &f.Admin, &f.Mutual); err != nil {
			return out, err
		}
		out.Friends = append(out.Friends, f)
	}
	if err := rows.Err(); err != nil {
		return out, err
	}
	rows2, err := s.db.Query(`
SELECT u.id, u.username || '#' || u.tag, u.avatar, u.is_admin FROM friends b JOIN users u ON u.id = b.user_id
WHERE b.friend_id = ? AND NOT EXISTS(SELECT 1 FROM friends a WHERE a.user_id = ? AND a.friend_id = b.user_id)
ORDER BY u.username COLLATE NOCASE`, user, user)
	if err != nil {
		return out, err
	}
	defer rows2.Close()
	for rows2.Next() {
		var f Friend
		if err := rows2.Scan(&f.ID, &f.Name, &f.Avatar, &f.Admin); err != nil {
			return out, err
		}
		out.Requests = append(out.Requests, f)
	}
	return out, rows2.Err()
}

// Mutual reports whether a and b added each other.
func (s *Store) Mutual(a, b int64) (bool, error) {
	var n int
	err := s.db.QueryRow(`SELECT count(*) FROM friends WHERE (user_id = ? AND friend_id = ?) OR (user_id = ? AND friend_id = ?)`,
		a, b, b, a).Scan(&n)
	return n == 2, err
}

// MutualIDs lists the accounts that added user back, by id (presence fan-out).
func (s *Store) MutualIDs(user int64) ([]int64, error) {
	rows, err := s.db.Query(`SELECT a.friend_id FROM friends a JOIN friends b ON b.user_id = a.friend_id AND b.friend_id = a.user_id WHERE a.user_id = ?`, user)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []int64
	for rows.Next() {
		var id int64
		if err := rows.Scan(&id); err != nil {
			return nil, err
		}
		out = append(out, id)
	}
	return out, rows.Err()
}
