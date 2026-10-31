// Package store is the Santral's SQLite database (docs/design/accounts.md §5):
// accounts (one-time invite codes, argon2id passwords, opaque session tokens) and the
// append-only events log that later statistics are computed from.
package store

import (
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"database/sql"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"strings"
	"sync"
	"time"
	"unicode/utf8"

	"golang.org/x/crypto/argon2"
	_ "modernc.org/sqlite"
)

// Error is a client-facing failure; Code() is the protocol `error.code`.
type Error string

func (e Error) Error() string { return string(e) }
func (e Error) Code() string  { return string(e) }

const (
	ErrBadCode        Error = "bad_code"     // invite code unknown, used or expired
	ErrBadUsername    Error = "bad_username" // 2–24 characters
	ErrBadPassword    Error = "bad_password" // 8–128 characters
	ErrTaken          Error = "username_taken"
	ErrBadLogin       Error = "bad_login" // wrong username or password
	ErrBadToken       Error = "bad_token" // unknown or expired session
	ErrTooManyInvites Error = "too_many_invites"
)

// SessionIdle: a session token unused this long stops working.
const SessionIdle = 90 * 24 * time.Hour

const schema = `
CREATE TABLE IF NOT EXISTS users (
	id         INTEGER PRIMARY KEY,
	username   TEXT NOT NULL COLLATE NOCASE,
	tag        TEXT NOT NULL,
	avatar     TEXT NOT NULL DEFAULT '',
	pass_hash  TEXT NOT NULL,
	created_at INTEGER NOT NULL,
	is_admin   INTEGER NOT NULL DEFAULT 0,
	UNIQUE (username, tag)
);
CREATE TABLE IF NOT EXISTS sessions (
	token_hash   TEXT PRIMARY KEY,
	user_id      INTEGER NOT NULL REFERENCES users(id),
	created_at   INTEGER NOT NULL,
	last_seen    INTEGER NOT NULL,
	device_label TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS invites (
	token_hash TEXT PRIMARY KEY,
	created_by INTEGER REFERENCES users(id),
	used_by    INTEGER REFERENCES users(id),
	expires_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS events (
	id      INTEGER PRIMARY KEY,
	ts      INTEGER NOT NULL,
	room_id TEXT NOT NULL,
	user_id INTEGER REFERENCES users(id),
	type    TEXT NOT NULL,
	payload TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS events_user_type ON events(user_id, type);
CREATE TABLE IF NOT EXISTS friends (
	user_id    INTEGER NOT NULL REFERENCES users(id),
	friend_id  INTEGER NOT NULL REFERENCES users(id),
	created_at INTEGER NOT NULL,
	PRIMARY KEY (user_id, friend_id)
);
`

// User is an account. Names may repeat; Name#Tag is unique (Handle).
type User struct {
	ID     int64
	Name   string
	Tag    string
	Avatar string
	Admin  bool
	// CanInvite: false after an admin banned someone this member invited.
	CanInvite bool
}

// Handle is the unique "Name#1234" form people use to find each other.
func (u User) Handle() string { return u.Name + "#" + u.Tag }

// Store owns the database and the events writer.
type Store struct {
	db     *sql.DB
	events chan event
	wg     sync.WaitGroup
	now    func() time.Time

	mu     sync.RWMutex // guards closed against Record racing Close
	closed bool
}

type event struct {
	ts      int64
	room    string
	user    int64
	typ     string
	payload any
}

// Open opens (creating if needed) the database at path.
func Open(path string) (*Store, error) {
	dsn := "file:" + path + "?_pragma=journal_mode(WAL)&_pragma=busy_timeout(5000)&_pragma=foreign_keys(1)&_pragma=synchronous(NORMAL)"
	db, err := sql.Open("sqlite", dsn)
	if err != nil {
		return nil, err
	}
	db.SetMaxOpenConns(1) // one writer; the load is a handful of friends
	if err := migrateTags(db); err != nil {
		db.Close()
		return nil, fmt.Errorf("migrate: %w", err)
	}
	if _, err := db.Exec(schema + listsSchema); err != nil {
		db.Close()
		return nil, fmt.Errorf("schema: %w", err)
	}
	if err := migrateAdmin(db); err != nil {
		db.Close()
		return nil, fmt.Errorf("migrate: %w", err)
	}
	s := &Store{db: db, events: make(chan event, 4096), now: time.Now}
	s.wg.Add(1)
	go s.writeEvents()
	return s, nil
}

// Close flushes pending events and closes the database. Later Record calls are dropped.
func (s *Store) Close() error {
	s.mu.Lock()
	s.closed = true
	close(s.events)
	s.mu.Unlock()
	s.wg.Wait()
	return s.db.Close()
}

// migrateTags rebuilds a pre-p0.18 users table (UNIQUE username, no tag/avatar) into
// the tagged one: each existing account gets a random #tag (SQLite's documented
// table rebuild, foreign keys off while the table is swapped).
func migrateTags(db *sql.DB) error {
	var n int
	if err := db.QueryRow(`SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'users'`).Scan(&n); err != nil || n == 0 {
		return err
	}
	if err := db.QueryRow(`SELECT count(*) FROM pragma_table_info('users') WHERE name = 'tag'`).Scan(&n); err != nil || n > 0 {
		return err
	}
	if _, err := db.Exec(`PRAGMA foreign_keys = OFF`); err != nil {
		return err
	}
	defer db.Exec(`PRAGMA foreign_keys = ON`)
	tx, err := db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	for _, q := range []string{
		`CREATE TABLE users_new (
	id         INTEGER PRIMARY KEY,
	username   TEXT NOT NULL COLLATE NOCASE,
	tag        TEXT NOT NULL,
	avatar     TEXT NOT NULL DEFAULT '',
	pass_hash  TEXT NOT NULL,
	created_at INTEGER NOT NULL,
	is_admin   INTEGER NOT NULL DEFAULT 0,
	UNIQUE (username, tag)
)`,
		`INSERT INTO users_new(id, username, tag, pass_hash, created_at, is_admin)
		 SELECT id, username, printf('%04d', abs(random()) % 10000), pass_hash, created_at, is_admin FROM users`,
		`DROP TABLE users`,
		`ALTER TABLE users_new RENAME TO users`,
	} {
		if _, err := tx.Exec(q); err != nil {
			return err
		}
	}
	return tx.Commit()
}

// newTag picks a #tag not yet used with name.
func newTag(q interface {
	QueryRow(string, ...any) *sql.Row
}, name string) (string, error) {
	for range 50 {
		b := make([]byte, 2)
		if _, err := rand.Read(b); err != nil {
			return "", err
		}
		tag := fmt.Sprintf("%04d", (int(b[0])<<8|int(b[1]))%10000)
		var n int
		if err := q.QueryRow(`SELECT count(*) FROM users WHERE username = ? AND tag = ?`, name, tag).Scan(&n); err != nil {
			return "", err
		}
		if n == 0 {
			return tag, nil
		}
	}
	return "", ErrTaken
}

// SplitHandle parses "Name#1234"; tag is "" when there is none.
func SplitHandle(h string) (name, tag string) {
	h = strings.TrimSpace(h)
	if i := strings.LastIndexByte(h, '#'); i > 0 {
		return strings.TrimSpace(h[:i]), strings.TrimSpace(h[i+1:])
	}
	return h, ""
}

// Lookup finds an account by "Name#1234" (or a bare name only one account has).
func (s *Store) Lookup(handle string) (User, error) {
	u, _, err := s.lookup(handle)
	return u, err
}

// UserByID loads an account.
func (s *Store) UserByID(id int64) (User, error) {
	var u User
	err := s.db.QueryRow(`SELECT id, username, tag, avatar, is_admin, can_invite FROM users WHERE id = ?`, id).Scan(&u.ID, &u.Name, &u.Tag, &u.Avatar, &u.Admin, &u.CanInvite)
	if errors.Is(err, sql.ErrNoRows) {
		return User{}, ErrUnknownUser
	}
	return u, err
}

// lookup finds an account by "Name#1234", or by bare name when only one account has it.
func (s *Store) lookup(handle string) (User, string, error) {
	name, tag := SplitHandle(handle)
	q := `SELECT id, username, tag, avatar, pass_hash FROM users WHERE username = ?`
	args := []any{name}
	if tag != "" {
		q += ` AND tag = ?`
		args = append(args, tag)
	}
	rows, err := s.db.Query(q+` LIMIT 2`, args...)
	if err != nil {
		return User{}, "", err
	}
	defer rows.Close()
	var u User
	var hash string
	found := 0
	for rows.Next() {
		if err := rows.Scan(&u.ID, &u.Name, &u.Tag, &u.Avatar, &hash); err != nil {
			return User{}, "", err
		}
		found++
	}
	if err := rows.Err(); err != nil {
		return User{}, "", err
	}
	if found != 1 {
		return User{}, "", ErrUnknownUser
	}
	return u, hash, nil
}

func random(n int) string {
	b := make([]byte, n)
	if _, err := rand.Read(b); err != nil {
		panic(err)
	}
	return base64.RawURLEncoding.EncodeToString(b)
}

func hashToken(t string) string {
	h := sha256.Sum256([]byte(t))
	return hex.EncodeToString(h[:])
}

// inviteAlphabet: no 0/O, 1/I lookalikes; codes read "ABCD-EFGH".
const inviteAlphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"

// normInvite drops dashes/spaces and upper-cases (Turkish İ → I), so typed codes match.
func normInvite(c string) string {
	var b strings.Builder
	for _, r := range strings.ToUpper(strings.ReplaceAll(c, "İ", "I")) {
		if strings.ContainsRune(inviteAlphabet, r) {
			b.WriteRune(r)
		}
	}
	return b.String()
}

// CreateInvite returns a one-time registration code valid for ttl. createdBy 0 = admin
// CLI; otherwise whoever registers with it becomes createdBy's friend.
func (s *Store) CreateInvite(createdBy int64, ttl time.Duration) (string, error) {
	b := make([]byte, 8)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	raw := make([]byte, 8)
	for i, x := range b {
		raw[i] = inviteAlphabet[int(x)%len(inviteAlphabet)]
	}
	var by any
	if createdBy != 0 {
		by = createdBy
		var can bool
		if err := s.db.QueryRow(`SELECT can_invite OR is_admin FROM users WHERE id = ?`, createdBy).Scan(&can); err != nil {
			return "", err
		}
		if !can {
			return "", ErrNoInvites
		}
		var open int
		if err := s.db.QueryRow(`SELECT count(*) FROM invites WHERE created_by = ? AND used_by IS NULL AND expires_at > ?`,
			createdBy, s.now().UnixMilli()).Scan(&open); err != nil {
			return "", err
		}
		if open >= MaxOpenInvites {
			return "", ErrTooManyInvites
		}
	}
	_, err := s.db.Exec(`INSERT INTO invites(token_hash, created_by, expires_at) VALUES(?,?,?)`,
		hashToken(string(raw)), by, s.now().Add(ttl).UnixMilli())
	return string(raw[:4]) + "-" + string(raw[4:]), err
}

// MaxOpenInvites: unused, unexpired codes one member may hold.
const MaxOpenInvites = 5

// Invite is one of a member's codes as the profile shows it (the code itself is not
// stored, only its hash).
type Invite struct {
	Expires int64  `json:"expires"`
	UsedBy  string `json:"usedBy,omitempty"`
	Avatar  string `json:"avatar,omitempty"`
}

// Invites lists user's last 10 codes, newest first.
func (s *Store) Invites(user int64) ([]Invite, error) {
	rows, err := s.db.Query(`SELECT i.expires_at, COALESCE(u.username || '#' || u.tag, ''), COALESCE(u.avatar, '')
FROM invites i LEFT JOIN users u ON u.id = i.used_by WHERE i.created_by = ? ORDER BY i.rowid DESC LIMIT 10`, user)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []Invite{}
	for rows.Next() {
		var v Invite
		if err := rows.Scan(&v.Expires, &v.UsedBy, &v.Avatar); err != nil {
			return nil, err
		}
		out = append(out, v)
	}
	return out, rows.Err()
}

// SetAvatar stores the chosen picture id (the hub checks it is one of the server's).
func (s *Store) SetAvatar(user int64, avatar string) error {
	_, err := s.db.Exec(`UPDATE users SET avatar = ? WHERE id = ?`, avatar, user)
	return err
}

// argon2id parameters (RFC 9106 §4, second recommended option).
const (
	argonTime    = 3
	argonMemory  = 64 * 1024
	argonThreads = 4
	argonKeyLen  = 32
)

func hashPassword(pw string) string {
	salt := make([]byte, 16)
	if _, err := rand.Read(salt); err != nil {
		panic(err)
	}
	key := argon2.IDKey([]byte(pw), salt, argonTime, argonMemory, argonThreads, argonKeyLen)
	b64 := base64.RawStdEncoding
	return fmt.Sprintf("$argon2id$v=%d$m=%d,t=%d,p=%d$%s$%s",
		argon2.Version, argonMemory, argonTime, argonThreads, b64.EncodeToString(salt), b64.EncodeToString(key))
}

func checkPassword(encoded, pw string) bool {
	parts := strings.Split(encoded, "$")
	if len(parts) != 6 || parts[1] != "argon2id" {
		return false
	}
	var m, t uint32
	var p uint8
	if _, err := fmt.Sscanf(parts[3], "m=%d,t=%d,p=%d", &m, &t, &p); err != nil {
		return false
	}
	b64 := base64.RawStdEncoding
	salt, err1 := b64.DecodeString(parts[4])
	want, err2 := b64.DecodeString(parts[5])
	if err1 != nil || err2 != nil {
		return false
	}
	got := argon2.IDKey([]byte(pw), salt, t, m, p, uint32(len(want)))
	return subtle.ConstantTimeCompare(got, want) == 1
}

// dummyHash keeps login timing the same for unknown usernames.
var dummyHash = hashPassword("koni-dummy-password")

// ValidUsername trims and checks 2–24 characters, no control characters.
func ValidUsername(name string) (string, bool) {
	name = strings.TrimSpace(name)
	n := utf8.RuneCountInString(name)
	if n < 2 || n > 24 {
		return "", false
	}
	for _, c := range name {
		if c < 0x20 || c == 0x7f {
			return "", false
		}
	}
	return name, true
}

func validPassword(pw string) bool {
	n := utf8.RuneCountInString(pw)
	return n >= 8 && n <= 128
}

// Register consumes an invite code and creates the account. The first account is admin.
// Returns the user and a new session token.
func (s *Store) Register(code, username, password, device, avatar string) (User, string, error) {
	name, ok := ValidUsername(username)
	if !ok {
		return User{}, "", ErrBadUsername
	}
	if !validPassword(password) {
		return User{}, "", ErrBadPassword
	}
	hash := hashPassword(password) // before the transaction: ~100 ms
	tx, err := s.db.Begin()
	if err != nil {
		return User{}, "", err
	}
	defer tx.Rollback()
	now := s.now().UnixMilli()
	var expires int64
	var used, by sql.NullInt64
	codeHash := hashToken(code)
	err = tx.QueryRow(`SELECT expires_at, used_by, created_by FROM invites WHERE token_hash = ?`, codeHash).Scan(&expires, &used, &by)
	if errors.Is(err, sql.ErrNoRows) { // member codes are stored normalized ("ABCD-EFGH" → "ABCDEFGH")
		codeHash = hashToken(normInvite(code))
		err = tx.QueryRow(`SELECT expires_at, used_by, created_by FROM invites WHERE token_hash = ?`, codeHash).Scan(&expires, &used, &by)
	}
	if errors.Is(err, sql.ErrNoRows) || (err == nil && (used.Valid || expires < now)) {
		return User{}, "", ErrBadCode
	}
	if err != nil {
		return User{}, "", err
	}
	var users int
	if err := tx.QueryRow(`SELECT count(*) FROM users`).Scan(&users); err != nil {
		return User{}, "", err
	}
	tag, err := newTag(tx, name)
	if err != nil {
		return User{}, "", err
	}
	res, err := tx.Exec(`INSERT INTO users(username, tag, avatar, pass_hash, created_at, is_admin) VALUES(?,?,?,?,?,?)`,
		name, tag, avatar, hash, now, users == 0)
	if err != nil {
		if strings.Contains(err.Error(), "UNIQUE") {
			return User{}, "", ErrTaken
		}
		return User{}, "", err
	}
	id, _ := res.LastInsertId()
	if _, err := tx.Exec(`UPDATE invites SET used_by = ? WHERE token_hash = ?`, id, codeHash); err != nil {
		return User{}, "", err
	}
	// A member's code: inviter and newcomer are friends right away (both directions).
	if by.Valid {
		for _, pair := range [][2]int64{{by.Int64, id}, {id, by.Int64}} {
			if _, err := tx.Exec(`INSERT OR IGNORE INTO friends(user_id, friend_id, created_at) VALUES(?,?,?)`, pair[0], pair[1], now); err != nil {
				return User{}, "", err
			}
		}
	}
	tok, err := newSession(tx, id, device, now)
	if err != nil {
		return User{}, "", err
	}
	if err := tx.Commit(); err != nil {
		return User{}, "", err
	}
	return User{ID: id, Name: name, Tag: tag, Avatar: avatar}, tok, nil
}

type execer interface {
	Exec(query string, args ...any) (sql.Result, error)
}

func newSession(db execer, user int64, device string, now int64) (string, error) {
	tok := random(32)
	if utf8.RuneCountInString(device) > 64 {
		device = string([]rune(device)[:64])
	}
	_, err := db.Exec(`INSERT INTO sessions(token_hash, user_id, created_at, last_seen, device_label) VALUES(?,?,?,?,?)`,
		hashToken(tok), user, now, now, device)
	return tok, err
}

// Login checks the password and opens a new session.
func (s *Store) Login(username, password, device string) (User, string, error) {
	// "Ad#1234", or the bare name while only one account has it.
	u, hash, err := s.lookup(username)
	if errors.Is(err, ErrUnknownUser) {
		checkPassword(dummyHash, password)
		return User{}, "", ErrBadLogin
	}
	if err != nil {
		return User{}, "", err
	}
	if !checkPassword(hash, password) {
		return User{}, "", ErrBadLogin
	}
	tok, err := newSession(s.db, u.ID, device, s.now().UnixMilli())
	if err != nil {
		return User{}, "", err
	}
	return u, tok, nil
}

// UserByToken resolves a session token and refreshes its last use.
func (s *Store) UserByToken(tok string) (User, error) {
	if tok == "" {
		return User{}, ErrBadToken
	}
	now := s.now()
	var u User
	var last int64
	h := hashToken(tok)
	err := s.db.QueryRow(`SELECT u.id, u.username, u.tag, u.avatar, u.is_admin, u.can_invite, s.last_seen FROM sessions s JOIN users u ON u.id = s.user_id
		WHERE s.token_hash = ?`, h).Scan(&u.ID, &u.Name, &u.Tag, &u.Avatar, &u.Admin, &u.CanInvite, &last)
	if errors.Is(err, sql.ErrNoRows) || (err == nil && now.Sub(time.UnixMilli(last)) > SessionIdle) {
		return User{}, ErrBadToken
	}
	if err != nil {
		return User{}, err
	}
	if _, err := s.db.Exec(`UPDATE sessions SET last_seen = ? WHERE token_hash = ?`, now.UnixMilli(), h); err != nil {
		return User{}, err
	}
	return u, nil
}

// Logout deletes the session.
func (s *Store) Logout(tok string) error {
	_, err := s.db.Exec(`DELETE FROM sessions WHERE token_hash = ?`, hashToken(tok))
	return err
}

// Record queues an event; it never blocks the caller (a full queue drops the event).
// user 0 means no account (system event).
func (s *Store) Record(room string, user int64, typ string, payload any) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	if s.closed {
		return
	}
	select {
	case s.events <- event{ts: s.now().UnixMilli(), room: room, user: user, typ: typ, payload: payload}:
	default:
		log.Printf("store: event queue full, dropped %s", typ)
	}
}

// writeEvents batches queued events into one transaction about once a second.
func (s *Store) writeEvents() {
	defer s.wg.Done()
	tick := time.NewTicker(time.Second)
	defer tick.Stop()
	var batch []event
	for {
		select {
		case e, ok := <-s.events:
			if !ok {
				s.flush(batch)
				return
			}
			batch = append(batch, e)
			if len(batch) < 512 {
				continue
			}
		case <-tick.C:
		}
		s.flush(batch)
		batch = batch[:0]
	}
}

func (s *Store) flush(batch []event) {
	if len(batch) == 0 {
		return
	}
	tx, err := s.db.Begin()
	if err != nil {
		log.Printf("store: events: %v", err)
		return
	}
	defer tx.Rollback()
	for _, e := range batch {
		p := []byte("{}")
		if e.payload != nil {
			if b, err := json.Marshal(e.payload); err == nil {
				p = b
			}
		}
		var user any
		if e.user != 0 {
			user = e.user
		}
		if _, err := tx.Exec(`INSERT INTO events(ts, room_id, user_id, type, payload) VALUES(?,?,?,?,?)`,
			e.ts, e.room, user, e.typ, string(p)); err != nil {
			log.Printf("store: events: %v", err)
			return
		}
	}
	if err := tx.Commit(); err != nil {
		log.Printf("store: events: %v", err)
	}
}
