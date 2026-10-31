package store

import (
	"database/sql"
	"errors"
)

// ErrNoInvites: an admin took this member's invite right (someone they invited was banned).
const ErrNoInvites Error = "no_invites"

// migrateAdmin adds the invite-right column to a p0.18 users table.
func migrateAdmin(db *sql.DB) error {
	var n int
	if err := db.QueryRow(`SELECT count(*) FROM pragma_table_info('users') WHERE name = 'can_invite'`).Scan(&n); err != nil || n > 0 {
		return err
	}
	_, err := db.Exec(`ALTER TABLE users ADD COLUMN can_invite INTEGER NOT NULL DEFAULT 1`)
	return err
}

// IsAdmin reports whether user is an admin.
func (s *Store) IsAdmin(user int64) (bool, error) {
	var a bool
	err := s.db.QueryRow(`SELECT is_admin FROM users WHERE id = ?`, user).Scan(&a)
	if errors.Is(err, sql.ErrNoRows) {
		return false, ErrUnknownUser
	}
	return a, err
}

// AdminIDs: every admin account, for the frame other people see.
func (s *Store) AdminIDs() (map[int64]bool, error) {
	rows, err := s.db.Query(`SELECT id FROM users WHERE is_admin = 1`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := map[int64]bool{}
	for rows.Next() {
		var id int64
		if err := rows.Scan(&id); err != nil {
			return nil, err
		}
		out[id] = true
	}
	return out, rows.Err()
}

// SetAdmin makes handle an admin (or not). Used by the `koni-server admin` command.
func (s *Store) SetAdmin(handle string, on bool) (User, error) {
	u, err := s.Lookup(handle)
	if err != nil {
		return u, err
	}
	_, err = s.db.Exec(`UPDATE users SET is_admin = ? WHERE id = ?`, on, u.ID)
	return u, err
}

// SetInviteRight gives a member's invite right back (or takes it).
func (s *Store) SetInviteRight(user int64, on bool) error {
	_, err := s.db.Exec(`UPDATE users SET can_invite = ? WHERE id = ?`, on, user)
	return err
}

// Member is one account as the admin screen lists it.
type Member struct {
	ID        int64  `json:"-"`
	Name      string `json:"name"` // "Ad#1234"
	Avatar    string `json:"avatar"`
	Admin     bool   `json:"admin,omitempty"`
	CanInvite bool   `json:"canInvite"`
	InvitedBy string `json:"invitedBy,omitempty"`
	Created   int64  `json:"created"`
	Friends   int    `json:"friends"` // mutual
}

// Members lists every account, newest first.
func (s *Store) Members() ([]Member, error) {
	rows, err := s.db.Query(`
SELECT u.id, u.username || '#' || u.tag, u.avatar, u.is_admin, u.can_invite, u.created_at,
	COALESCE((SELECT b.username || '#' || b.tag FROM invites i JOIN users b ON b.id = i.created_by WHERE i.used_by = u.id), ''),
	(SELECT count(*) FROM friends a JOIN friends c ON c.user_id = a.friend_id AND c.friend_id = a.user_id WHERE a.user_id = u.id)
FROM users u ORDER BY u.created_at DESC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []Member{}
	for rows.Next() {
		var m Member
		if err := rows.Scan(&m.ID, &m.Name, &m.Avatar, &m.Admin, &m.CanInvite, &m.Created, &m.InvitedBy, &m.Friends); err != nil {
			return nil, err
		}
		out = append(out, m)
	}
	return out, rows.Err()
}

// Ban deletes user's account with its sessions, friendships, invites and statistics.
// The member who invited them loses the invite right (and their open codes) unless
// they are an admin. Returns that inviter's id (0 if none or an admin).
func (s *Store) Ban(user int64) (int64, error) {
	tx, err := s.db.Begin()
	if err != nil {
		return 0, err
	}
	defer tx.Rollback()
	var inviter int64
	var admin bool
	err = tx.QueryRow(`SELECT u.id, u.is_admin FROM invites i JOIN users u ON u.id = i.created_by WHERE i.used_by = ?`, user).Scan(&inviter, &admin)
	if err != nil && !errors.Is(err, sql.ErrNoRows) {
		return 0, err
	}
	if admin {
		inviter = 0
	}
	for _, q := range []string{
		`DELETE FROM sessions WHERE user_id = ?1`,
		`DELETE FROM friends WHERE user_id = ?1 OR friend_id = ?1`,
		`DELETE FROM events WHERE user_id = ?1`,
		`DELETE FROM invites WHERE used_by = ?1 OR created_by = ?1`,
		`DELETE FROM users WHERE id = ?1`,
	} {
		if _, err := tx.Exec(q, user); err != nil {
			return 0, err
		}
	}
	if inviter != 0 {
		if _, err := tx.Exec(`UPDATE users SET can_invite = 0 WHERE id = ?`, inviter); err != nil {
			return 0, err
		}
		if _, err := tx.Exec(`DELETE FROM invites WHERE created_by = ? AND used_by IS NULL`, inviter); err != nil {
			return 0, err
		}
	}
	return inviter, tx.Commit()
}

// AdminAvatars: pictures worn by admins other than user; nobody else may pick them.
func (s *Store) AdminAvatars(user int64) ([]string, error) {
	rows, err := s.db.Query(`SELECT avatar FROM users WHERE is_admin = 1 AND avatar != '' AND id != ?`, user)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []string{}
	for rows.Next() {
		var a string
		if err := rows.Scan(&a); err != nil {
			return nil, err
		}
		out = append(out, a)
	}
	return out, rows.Err()
}
