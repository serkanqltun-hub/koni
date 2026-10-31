package hub

import (
	"crypto/subtle"
	"fmt"
	"math"
	"net/url"
	"strings"
	"sync"
	"time"
	"unicode/utf8"

	"github.com/serkanqltun-hub/koni/server/discover"
	"github.com/serkanqltun-hub/koni/server/store"
	"github.com/serkanqltun-hub/koni/server/subs"
)

const (
	PolicyFree     = "free"
	PolicyHostOnly = "host_only"

	// Room view (p0.13): what the room is doing; every client shows the same screen.
	ViewBrowse = "browse" // deciding what to watch ("Ne izlesek?")
	ViewWatch  = "watch"  // cinema
	ViewListen = "listen" // music (Lounge queue)

	ModePaused    = "paused"
	ModeCountdown = "countdown"
	ModePlaying   = "playing"
	ModeHold      = "hold"

	BufferTargetSec = 12.0 // hysteresis reserve (docs/design/sync.md §4.7)
	MaxMembers      = 8
	cmdRateLimit    = 3
	statusInterval  = time.Second

	// chronic stall: this many stalls inside the window → offer "continue without" to the host
	chronicStalls = 3
	chronicWindow = 4 * time.Minute
)

// Config tunes timing; zero values pick production defaults.
type Config struct {
	MinLead   time.Duration // minimum countdown lead (default 250 ms)
	GhostWait time.Duration // how long a dropped member may resume (default 60 s)
	// QueueStart: a song the queue switched to starts this long after the switch, so
	// every client can resolve and load it first (default 3 s).
	QueueStart time.Duration
	// EmptyWait: a room nobody is in stays open this long, so its invite still
	// works from "Son odalar" (default 6 h).
	EmptyWait time.Duration
	// ReadyLead: the first start of a media ("Hazır mısınız?" 3-2-1) gets at least
	// this much countdown lead (default 3 s).
	ReadyLead time.Duration
	// Reports: directory bug reports are written to; "" turns bug_report off.
	Reports string
	Now     func() time.Time
	// Store enables accounts (create_room/join need a session token) and the events
	// log. nil: open server, anyone with a username, nothing persisted.
	Store *store.Store
	// Discover (TMDB/OMDb) and Subs (OpenSubtitles) are optional; nil turns the
	// feature off and the room answers with a fun_error.
	Discover *discover.Client
	Subs     *subs.Client
	// Avatars: directory of the profile pictures members may pick ("" turns them off).
	Avatars string
}

func (c Config) withDefaults() Config {
	if c.MinLead == 0 {
		c.MinLead = 250 * time.Millisecond
	}
	if c.GhostWait == 0 {
		c.GhostWait = 60 * time.Second
	}
	if c.QueueStart == 0 {
		c.QueueStart = 500 * time.Millisecond
	}
	if c.EmptyWait == 0 {
		c.EmptyWait = 6 * time.Hour
	}
	if c.ReadyLead == 0 {
		c.ReadyLead = 3 * time.Second
	}
	if c.Now == nil {
		c.Now = time.Now
	}
	return c
}

// Member is one participant. All fields are owned by the room goroutine.
type Member struct {
	ID     string
	Name   string
	Host   bool
	CoHost bool  // host-appointed; controls playback even in host_only
	User   int64 // account id; 0 on an open server
	Admin  bool  // server admin: shown with a frame

	send   chan []byte // current connection's queue; closed when that connection ends
	closed bool        // left the room for good
	ghost  bool        // connection lost; may resume with token until GhostWait
	gen    int         // bumped per connection, invalidates stale ghost timers
	token  string      // resume secret

	cmdTimes   []time.Time
	rtt        time.Duration
	lastStatus time.Time
	lastReact  time.Time
	waiting    bool

	stalls   []time.Time // recent stalls, for the chronic-stall offer
	offered  bool        // host already got a detach_offer for the current streak
	detached bool        // room continues without this member until it resyncs

	// Watch time not yet written as a "watch" event, and this presence's total (ms).
	watchMs, nightMs, sessionMs int64
	// Pending Lounge listening time (queue songs), written as a "listen" event.
	listenMs int64
}

func (m *Member) info() MemberInfo {
	return MemberInfo{ID: m.ID, Name: m.Name, Host: m.Host, CoHost: m.CoHost, Admin: m.Admin, Away: m.ghost, Detached: m.detached}
}

// canControl: host or co-host (host_only playback, continue_without, detach offers).
func (m *Member) canControl() bool { return m.Host || m.CoHost }

// Room is an actor: all state is mutated only inside run().
type Room struct {
	id      string
	name    string
	invite  string
	cfg     Config
	onEmpty func(*Room)

	inbox chan func()
	done  chan struct{}
	stop  sync.Once

	created  int64 // unix ms
	policy   string
	view     string
	members  map[string]*Member
	order    []string // join order, for host handover
	nextID   int
	emptyGen int // bumped when the room empties or someone joins; stale close timers skip

	seq       int64
	mode      string
	anchorPos float64
	anchorTs  int64 // ms
	startAt   int64 // ms
	waiting   []string
	timer     *time.Timer
	media     *Media // replaced, never mutated: safe to share across messages
	started   bool   // media has reached PLAYING at least once
	dl        *dlTask

	lastAccrue int64 // ms; watch time is credited up to here

	// "Ne izlesek?" and room subtitles (fun.go)
	deck        *deckState
	deckSeq     int
	deckLoading bool
	seen        map[int]bool // TMDB ids already dealt in this room
	wheel       wheelState
	night       *nightState // night.go
	nightSeq    int
	subtitle    *roomSubtitle
	subsBusy    bool
	queue       queueState

	// Media already put on members' "İzlediklerim" (p0.23).
	seenDone map[*Media]bool

	// Accounts that were ever in the room → when they left (unix ms, 0 = inside).
	// They may come back without a call while the room is open (p0.22).
	alumni map[int64]int64
}

func newRoom(id, name, invite, policy string, cfg Config, onEmpty func(*Room)) *Room {
	r := &Room{
		id: id, name: name, invite: invite, cfg: cfg.withDefaults(), onEmpty: onEmpty,
		inbox: make(chan func(), 256), done: make(chan struct{}),
		policy: policy, view: ViewBrowse, members: map[string]*Member{}, mode: ModePaused, seen: map[int]bool{}, alumni: map[int64]int64{}, seenDone: map[*Media]bool{},
	}
	r.anchorTs = r.nowMs()
	r.lastAccrue = r.anchorTs
	r.created = r.anchorTs
	go r.run()
	return r
}

// watchFlush: pending watch time is written at least this often, so a crash loses little.
const watchFlush = 5 * time.Minute

func (r *Room) run() {
	flush := time.NewTicker(watchFlush)
	defer flush.Stop()
	for {
		select {
		case fn := <-r.inbox:
			r.accrue() // credit the time spent in the state before fn changes it
			r.markSeen()
			fn()
		case <-flush.C:
			r.accrue()
			r.markSeen()
			for _, m := range r.members {
				r.flushWatch(m)
			}
		case <-r.done:
			return
		}
	}
}

// accrue credits the time since the last call to every connected member while the
// room is playing (detached members keep watching on their own). Night time is the
// part between 00:00 and 04:00 Turkey time (docs/design/accounts.md §5.3–5.4).
func (r *Room) accrue() {
	now := r.nowMs()
	from := r.lastAccrue
	r.lastAccrue = now
	if r.mode != ModePlaying || now <= from {
		return
	}
	night := store.NightMs(from, now)
	music := r.media != nil && r.media == r.queue.playing
	for _, m := range r.members {
		if m.ghost || m.closed {
			continue
		}
		if music {
			m.listenMs += now - from
			continue
		}
		m.watchMs += now - from
		m.nightMs += night
		m.sessionMs += now - from
	}
}

// flushWatch writes m's pending watch (and listen) time; `session` is this presence's
// running watch total.
func (r *Room) flushWatch(m *Member) {
	if m.watchMs > 0 {
		ev := map[string]any{"ms": m.watchMs, "night": m.nightMs, "session": m.sessionMs}
		if r.media != nil && r.media.Title != "" {
			ev["title"] = r.media.Title // "Etkinlik" tab (p0.22)
		}
		r.record("watch", m, ev)
		m.watchMs, m.nightMs = 0, 0
	}
	if m.listenMs > 0 {
		r.record("listen", m, map[string]any{"ms": m.listenMs})
		m.listenMs = 0
	}
}

func (r *Room) nowMs() int64 { return r.cfg.Now().UnixMilli() }

// do schedules fn on the room goroutine; false if the room is gone.
func (r *Room) do(fn func()) bool {
	select {
	case <-r.done:
		return false
	default:
	}
	select {
	case r.inbox <- fn:
		return true
	case <-r.done:
		return false
	}
}

// call runs fn on the room goroutine and waits for it.
func (r *Room) call(fn func()) bool {
	fin := make(chan struct{})
	if !r.do(func() { fn(); close(fin) }) {
		return false
	}
	select {
	case <-fin:
		return true
	case <-r.done:
		// fn may still have run; check fin once more
		select {
		case <-fin:
			return true
		default:
			return false
		}
	}
}

// ---- membership -------------------------------------------------------

type joinResult struct {
	m     *Member
	out   chan []byte
	token string
	snap  Snapshot
	err   string
}

func validName(s string) (string, bool) {
	s = strings.TrimSpace(s)
	n := utf8.RuneCountInString(s)
	return s, n >= 1 && n <= 24
}

// join adds a member; must run on the room goroutine. host marks the creator.
func (r *Room) join(username string, user int64, host bool) joinResult {
	name, ok := validName(username)
	if !ok {
		return joinResult{err: "bad_username"}
	}
	if len(r.members) == 0 {
		host = true // rejoining an empty room: the first one in runs it
		r.emptyGen++
	}
	if len(r.members) >= MaxMembers {
		return joinResult{err: "room_full"}
	}
	for _, m := range r.members {
		if strings.EqualFold(m.Name, name) {
			return joinResult{err: "name_taken"}
		}
	}
	r.nextID++
	m := &Member{ID: fmt.Sprintf("u%d", r.nextID), Name: name, Host: host, User: user, send: make(chan []byte, 64), token: token(12)}
	if user != 0 && r.cfg.Store != nil {
		m.Admin, _ = r.cfg.Store.IsAdmin(user) // the admin frame (p0.19)
	}
	r.members[m.ID] = m
	r.order = append(r.order, m.ID)
	if user != 0 {
		r.alumni[user] = 0
	}
	if host && r.nextID == 1 {
		r.record("room_create", m, map[string]any{"name": r.name, "policy": r.policy})
	}
	r.broadcastExcept(m.ID, enc(map[string]any{"t": "member_joined", "member": m.info()}))
	r.activity("join", m.ID, m.Name+" odaya katıldı")
	r.sendFunState(m)
	r.afterVotes() // a new voter: progress counts change
	r.nightMembersChanged()
	return joinResult{m: m, out: m.send, token: m.token, snap: r.snapshot()}
}

// resume reattaches a ghost (or a member whose old connection is still half-open)
// to a new connection.
func (r *Room) resume(id, tok string) joinResult {
	m := r.members[id]
	if m == nil || m.closed || subtle.ConstantTimeCompare([]byte(tok), []byte(m.token)) != 1 {
		return joinResult{err: "resume_failed"}
	}
	if !m.ghost {
		close(m.send) // old connection's writer closes it; its Close is then ignored
	}
	m.ghost = false
	m.gen++
	m.send = make(chan []byte, 64)
	r.broadcastExcept(m.ID, enc(map[string]any{"t": "member_back", "id": m.ID}))
	r.activity("join", m.ID, m.Name+" geri bağlandı")
	r.sendFunState(m)
	r.nightMembersChanged()
	return joinResult{m: m, out: m.send, token: m.token, snap: r.snapshot()}
}

func (r *Room) snapshot() Snapshot {
	s := Snapshot{Room: r.id, Name: r.name, Policy: r.policy, View: r.view, State: r.stateMsg("snapshot", nil, ""), Downloads: r.downloads()}
	for _, id := range r.order {
		m := r.members[id]
		s.Members = append(s.Members, m.info())
		if m.Host {
			s.Host = m.ID
		}
	}
	return s
}

// disconnect turns a member whose connection (out) ended into a ghost; it leaves for
// good unless it resumes within GhostWait.
func (r *Room) disconnect(m *Member, out chan []byte) {
	if m.closed || m.ghost || m.send != out {
		return // already gone, or a newer connection took over
	}
	m.ghost = true
	close(m.send)
	m.gen++
	gen := m.gen
	time.AfterFunc(r.cfg.GhostWait, func() {
		r.do(func() {
			if m.ghost && m.gen == gen {
				r.leave(m)
			}
		})
	})
	r.broadcast(enc(map[string]any{"t": "member_away", "id": m.ID}))
	r.activity("away", m.ID, m.Name+" bağlantısı koptu")
	if r.dropWaiting(m.ID) && r.mode == ModeHold {
		r.afterWaitingChanged(nil)
	}
	r.checkBarrier()
}

func (r *Room) leave(m *Member) {
	if m.closed {
		return
	}
	m.closed = true
	if m.User != 0 {
		r.alumni[m.User] = r.nowMs()
	}
	r.flushWatch(m)
	r.record("leave", m, nil) // before removal: the last member's leave closes the room
	if !m.ghost {
		close(m.send)
	}
	delete(r.members, m.ID)
	for i, id := range r.order {
		if id == m.ID {
			r.order = append(r.order[:i], r.order[i+1:]...)
			break
		}
	}
	if len(r.members) == 0 {
		r.idle()
		return
	}
	left := map[string]any{"t": "member_left", "id": m.ID}
	var nh *Member
	if m.Host {
		nh = r.nextHost()
		nh.Host, nh.CoHost = true, false
		left["host"] = nh.ID
	}
	r.broadcast(enc(left))
	r.feed("leave", m.ID, m.Name+" odadan ayrıldı")
	if nh != nil {
		r.activity("policy", nh.ID, nh.Name+" yeni host")
	}
	if r.dropWaiting(m.ID) && r.mode == ModeHold {
		r.afterWaitingChanged(nil)
	}
	r.checkBarrier()
	r.afterVotes()
	r.nightMembersChanged()
}

// idle runs when the last member left: playback stops where it was and the room
// closes after EmptyWait unless someone comes back.
func (r *Room) idle() {
	r.cancelTimer()
	r.waiting = nil
	if r.mode != ModePaused {
		r.anchorPos = r.position(r.nowMs())
		r.mode = ModePaused
		r.anchorTs = r.nowMs()
		r.startAt = 0
		r.seq++
	}
	r.emptyGen++
	gen := r.emptyGen
	time.AfterFunc(r.cfg.EmptyWait, func() {
		r.do(func() {
			if len(r.members) == 0 && r.emptyGen == gen {
				r.shutdown()
			}
		})
	})
}

// closeByHost ends the room for everyone (p0.22): members get `room_ended` and
// go home; the room leaves "Açık odalar" at once.
func (r *Room) closeByHost(m *Member) {
	if !m.Host {
		r.sendTo(m, enc(map[string]any{"t": "denied", "reason": "not_host"}))
		return
	}
	msg := enc(errMsg("room_ended"))
	for _, x := range r.members {
		if !x.ghost && x != m {
			r.sendTo(x, msg)
		}
		r.leave(x)
	}
	r.shutdown()
}

func (r *Room) shutdown() {
	if r.timer != nil {
		r.timer.Stop()
	}
	r.stop.Do(func() { close(r.done) })
	if r.onEmpty != nil {
		go r.onEmpty(r)
	}
}

// ---- state machine ----------------------------------------------------

func (r *Room) position(nowMs int64) float64 {
	if r.mode == ModePlaying {
		return r.anchorPos + float64(nowMs-r.anchorTs)/1000
	}
	return r.anchorPos
}

func (r *Room) stateMsg(cause string, by *Member, ack string) StateMsg {
	st := StateMsg{
		T: "state", Seq: r.seq, Mode: r.mode, AnchorPos: r.anchorPos, AnchorTs: r.anchorTs,
		StartAt: r.startAt, Rate: 1.0, Cause: cause, Ack: ack, ServerTs: r.nowMs(), Media: r.media,
	}
	if by != nil {
		st.By = &ByInfo{ID: by.ID, Name: by.Name}
	}
	if r.mode == ModeHold {
		hp := r.anchorPos
		st.HoldPos = &hp
		st.Reason = "stall"
		st.WaitingFor = append([]string(nil), r.waiting...)
	}
	return st
}

// publish bumps seq and sends the new state to everyone; the sender's copy carries ack.
func (r *Room) publish(cause string, by *Member, ack string) {
	r.seq++
	common := enc(r.stateMsg(cause, by, ""))
	var withAck []byte
	if ack != "" && by != nil {
		withAck = enc(r.stateMsg(cause, by, ack))
	}
	for _, m := range r.members {
		if withAck != nil && m.ID == by.ID {
			r.sendTo(m, withAck)
		} else {
			r.sendTo(m, common)
		}
	}
}

func (r *Room) cancelTimer() {
	if r.timer != nil {
		r.timer.Stop()
		r.timer = nil
	}
}

func (r *Room) lead() time.Duration {
	lead := r.cfg.MinLead
	for _, m := range r.members {
		if !m.ghost && m.rtt > lead {
			lead = m.rtt
		}
	}
	return lead
}

func (r *Room) startCountdown(pos float64, cause string, by *Member, ack string) {
	r.cancelTimer()
	now := r.cfg.Now()
	lead := r.lead()
	if r.media != nil && !r.started && lead < r.cfg.ReadyLead {
		lead = r.cfg.ReadyLead // first start of this media: "Hazır mısınız?" 3-2-1
	}
	r.mode = ModeCountdown
	r.anchorPos = pos
	r.anchorTs = now.UnixMilli()
	r.startAt = now.Add(lead).UnixMilli()
	r.publish(cause, by, ack)
	seq := r.seq
	r.timer = time.AfterFunc(lead, func() {
		r.do(func() { r.begin(seq) })
	})
}

// begin flips COUNTDOWN → PLAYING if nothing superseded this countdown.
func (r *Room) begin(seq int64) {
	if r.mode != ModeCountdown || r.seq != seq {
		return
	}
	r.timer = nil
	r.mode = ModePlaying
	r.started = true
	r.anchorTs = r.startAt
	r.publish("start", nil, "")
}

func (r *Room) release(by *Member) {
	r.waiting = nil
	r.startCountdown(r.anchorPos, "release", by, "")
	if by == nil { // continue_without already announced itself
		r.activity("release", "", "Herkes hazır, devam ediliyor")
	}
}

// afterWaitingChanged runs in HOLD after someone left the waiting list.
func (r *Room) afterWaitingChanged(by *Member) {
	if len(r.waiting) == 0 {
		r.release(by)
	} else {
		r.publish("stall", nil, "") // updated waitingFor
	}
}

func (r *Room) dropWaiting(id string) bool {
	for i, w := range r.waiting {
		if w == id {
			r.waiting = append(r.waiting[:i], r.waiting[i+1:]...)
			return true
		}
	}
	return false
}

func (r *Room) isWaiting(id string) bool {
	for _, w := range r.waiting {
		if w == id {
			return true
		}
	}
	return false
}

func fmtPos(sec float64) string {
	if sec < 0 {
		sec = 0
	}
	t := int(math.Round(sec))
	h, m, s := t/3600, t/60%60, t%60
	if h > 0 {
		return fmt.Sprintf("%02d:%02d:%02d", h, m, s)
	}
	return fmt.Sprintf("%02d:%02d", m, s)
}

// ---- inbound handling -------------------------------------------------

func (r *Room) handle(m *Member, in Inbound) {
	if m.closed || m.ghost {
		return
	}
	switch in.T {
	case "cmd":
		r.handleCmd(m, in)
	case "report":
		r.handleReport(m, in)
	case "ping":
		if in.RttMs > 0 {
			m.rtt = time.Duration(min(in.RttMs, 2000)) * time.Millisecond
		}
		r.sendTo(m, enc(map[string]any{"t": "pong", "t0": in.T0, "serverTs": r.nowMs()}))
	case "react":
		r.handleReact(m, in)
	case "leave":
		r.leave(m)
	case "close_room":
		r.closeByHost(m)
	case "dl_progress":
		r.dlProgress(m, in)
	case "discover":
		r.handleDiscover(m, in)
	case "swipe":
		r.handleSwipe(m, in)
	case "propose":
		r.handlePropose(m, in)
	case "unpropose":
		r.handleUnpropose(m, in)
	case "spin":
		r.handleSpin(m)
	case "wheel_clear":
		r.handleWheelClear(m)
	case "night_start":
		r.handleNightStart(m, in)
	case "night_close":
		r.handleNightClose(m)
	case "night_ready":
		r.handleNightReady(m, in)
	case "night_suggest":
		r.handleNightSuggest(m, in)
	case "night_preview":
		r.handleNightPreview(m, in)
	case "night_search":
		r.handleNightSearch(m, in)
	case "night_detail":
		r.handleNightDetail(m, in)
	case "night_pick":
		r.handleNightPick(m, in)
	case "night_unpick":
		r.handleNightUnpick(m, in)
	case "night_done":
		r.handleNightDone(m, in)
	case "subs_search":
		r.handleSubsSearch(m, in)
	case "subs_pick":
		r.handleSubsPick(m, in)
	case "queue_add":
		r.handleQueueAdd(m, in)
	case "queue_vote":
		r.handleQueueVote(m, in)
	case "queue_remove":
		r.handleQueueRemove(m, in)
	case "queue_next":
		r.handleQueueNext(m, in)
	case "track_end":
		r.handleTrackEnd(m, in)
	default:
		r.sendTo(m, enc(map[string]any{"t": "error", "code": "bad_message", "message": "bilinmeyen mesaj tipi"}))
	}
}

func (r *Room) deny(m *Member, cid, reason string) {
	r.sendTo(m, enc(map[string]any{"t": "denied", "ack": cid, "reason": reason}))
}

func (r *Room) allowRate(m *Member) bool {
	now := r.cfg.Now()
	cut := now.Add(-time.Second)
	keep := m.cmdTimes[:0]
	for _, t := range m.cmdTimes {
		if t.After(cut) {
			keep = append(keep, t)
		}
	}
	m.cmdTimes = keep
	if len(m.cmdTimes) >= cmdRateLimit {
		return false
	}
	m.cmdTimes = append(m.cmdTimes, now)
	return true
}

func (r *Room) handleCmd(m *Member, in Inbound) {
	if !r.allowRate(m) {
		r.deny(m, in.Cid, "rate_limited")
		return
	}
	switch in.Op {
	case "set_policy":
		if !m.Host {
			r.deny(m, in.Cid, "not_host")
			return
		}
		if in.Policy != PolicyFree && in.Policy != PolicyHostOnly {
			r.deny(m, in.Cid, "bad_op")
			return
		}
		r.policy = in.Policy
		r.broadcast(enc(map[string]any{"t": "policy", "policy": r.policy, "ack": in.Cid}))
		r.activity("policy", m.ID, fmt.Sprintf("%s politikayı %s yaptı", m.Name, r.policy))
	case "set_view":
		if r.policy == PolicyHostOnly && !m.canControl() {
			r.deny(m, in.Cid, "host_only")
			return
		}
		if in.View != ViewBrowse && in.View != ViewWatch && in.View != ViewListen {
			r.deny(m, in.Cid, "bad_op")
			return
		}
		r.setView(in.View, m, in.Cid)
	case "set_cohost":
		r.setCoHost(m, in)
	case "play", "pause", "seek", "media", "close":
		if r.policy == PolicyHostOnly && !m.canControl() {
			r.deny(m, in.Cid, "host_only")
			return
		}
		if in.Op == "media" {
			r.mediaCmd(m, in)
		} else if in.Op == "close" {
			r.closeMedia(m, in.Cid)
		} else {
			r.playbackCmd(m, in)
		}
	case "continue_without":
		r.continueWithout(m, in)
	case "resync":
		r.resync(m, in)
	default:
		r.deny(m, in.Cid, "bad_op")
	}
}

// noteStall records a stall that put m on the waiting list; on the chronic threshold
// the host is offered to continue without m (docs/design/sync.md §4.7).
// startLoad: the first seconds of a media, when every player is still opening it
// (yt-dlp takes a few seconds, every queue song starts like this).
const startLoad = 2.0

// noteHold logs and counts a member's stall; a stall while the media is still opening
// is neither (otherwise every song change would print "X bekleniyor" and, after three
// songs, offer the host to continue without them).
func (r *Room) noteHold(m *Member, pos float64) {
	if pos < startLoad {
		return
	}
	r.activity("hold", m.ID, fmt.Sprintf("%s bekleniyor", m.Name))
	r.noteStall(m)
}

func (r *Room) noteStall(m *Member) {
	now := r.cfg.Now()
	keep := m.stalls[:0]
	for _, t := range m.stalls {
		if now.Sub(t) < chronicWindow {
			keep = append(keep, t)
		}
	}
	m.stalls = append(keep, now)
	if len(m.stalls) < chronicStalls || m.offered || m.Host {
		return
	}
	m.offered = true
	for _, h := range r.members {
		if h.canControl() && h != m && !h.ghost {
			r.sendTo(h, enc(map[string]any{"t": "detach_offer", "user": m.ID, "name": m.Name, "stalls": len(m.stalls)}))
		}
	}
}

// continueWithout (host or co-host) detaches a member: the room stops waiting for them
// until they resync themselves. A co-host cannot detach the host.
func (r *Room) continueWithout(m *Member, in Inbound) {
	if !m.canControl() {
		r.deny(m, in.Cid, "not_host")
		return
	}
	t := r.members[in.User]
	if t == nil || t == m || t.ghost || (t.Host && !m.Host) {
		r.deny(m, in.Cid, "bad_op")
		return
	}
	r.detach(t, m)
}

// detach: the room stops waiting for t until t resyncs or the media changes. by nil =
// the server decided (t's player could not open the media).
func (r *Room) detach(t, by *Member) {
	if !t.detached {
		t.detached = true
		t.offered = false
		t.stalls = nil
		out := map[string]any{"t": "detached"}
		if by != nil {
			out["by"] = by.ID
		}
		r.sendTo(t, enc(out))
		r.broadcast(enc(map[string]any{"t": "member_detached", "id": t.ID, "detached": true}))
		if by != nil {
			r.activity("detach", t.ID, fmt.Sprintf("%s, %s olmadan devam ediyor", by.Name, t.Name))
		} else {
			r.activity("detach", t.ID, fmt.Sprintf("%s bunu açamadı, oda onsuz devam ediyor", t.Name))
		}
	}
	if r.dropWaiting(t.ID) && r.mode == ModeHold {
		r.afterWaitingChanged(by)
	}
	r.checkBarrier()
}

// setCoHost (host only) appoints or removes a co-host. Idempotent.
func (r *Room) setCoHost(m *Member, in Inbound) {
	if !m.Host {
		r.deny(m, in.Cid, "not_host")
		return
	}
	t := r.members[in.User]
	if t == nil || t.Host {
		r.deny(m, in.Cid, "bad_op")
		return
	}
	if t.CoHost == in.On {
		return
	}
	t.CoHost = in.On
	r.broadcast(enc(map[string]any{"t": "member_role", "id": t.ID, "cohost": t.CoHost, "ack": in.Cid}))
	if t.CoHost {
		r.activity("role", t.ID, fmt.Sprintf("%s, %s kişisini yardımcı host yaptı", m.Name, t.Name))
	} else {
		r.activity("role", t.ID, fmt.Sprintf("%s, %s kişisinin yardımcı hostluğunu aldı", m.Name, t.Name))
	}
}

// nextHost picks the successor when the host leaves: a connected co-host, else any
// connected member, else the oldest member (join order).
func (r *Room) nextHost() *Member {
	var anyConnected *Member
	for _, id := range r.order {
		c := r.members[id]
		if c.ghost {
			continue
		}
		if c.CoHost {
			return c
		}
		if anyConnected == nil {
			anyConnected = c
		}
	}
	if anyConnected != nil {
		return anyConnected
	}
	return r.members[r.order[0]]
}

// resync brings a detached member back: from now on the room waits for them again.
// Idempotent; the sender always gets the current state.
func (r *Room) resync(m *Member, in Inbound) {
	if m.detached {
		m.detached = false
		m.stalls = nil
		r.broadcast(enc(map[string]any{"t": "member_detached", "id": m.ID, "detached": false}))
		r.activity("resync", m.ID, m.Name+" odayla yeniden senkronlandı")
	}
	r.sendTo(m, enc(r.stateMsg("snapshot", nil, in.Cid)))
}

// undetachAll: new media starts everyone afresh.
func (r *Room) undetachAll() {
	for _, m := range r.members {
		m.stalls, m.offered = nil, false
		if m.detached {
			m.detached = false
			r.broadcast(enc(map[string]any{"t": "member_detached", "id": m.ID, "detached": false}))
		}
	}
}

const (
	maxMediaSrc   = 2048
	maxMediaTitle = 200
	maxMediaFile  = 255
	maxMediaIdx   = 100000
)

// mediaCmd switches the room to new media: paused at 0, every hold/countdown dropped.
func (r *Room) mediaCmd(m *Member, in Inbound) {
	if in.Media != nil && in.Media.Src == "__koni_closed__" {
		r.closeMedia(m, in.Cid)
		return
	}
	md, ok := cleanMedia(in.Media)
	if !ok {
		r.deny(m, in.Cid, "bad_media")
		return
	}
	r.switchMedia(md, m, in.Cid)
}

// closeMedia empties the room (p0.21): the video stops for everyone without
// closing the room.
func (r *Room) closeMedia(m *Member, cid string) {
	if r.media == nil {
		r.deny(m, cid, "no_media")
		return
	}
	title := r.media.Title
	r.cancelTimer()
	r.dl = nil
	r.media = nil
	r.started = false
	r.waiting = nil
	r.undetachAll()
	r.mode = ModePaused
	r.anchorPos, r.anchorTs, r.startAt = 0, r.nowMs(), 0
	r.publish("media", m, cid)
	r.activity("media", m.ID, fmt.Sprintf("%s kapattı: %s", m.Name, title))
}

// setView changes what every client shows; by nil = the server switched it.
func (r *Room) setView(v string, by *Member, cid string) {
	if r.view == v {
		if cid != "" && by != nil {
			r.broadcast(enc(map[string]any{"t": "view", "view": v, "ack": cid}))
		}
		return
	}
	r.view = v
	msg := map[string]any{"t": "view", "view": v}
	if cid != "" {
		msg["ack"] = cid
	}
	if by != nil {
		msg["by"] = by.Name
		names := map[string]string{ViewBrowse: "Keşfet", ViewWatch: "Sinema", ViewListen: "Müzik"}
		r.activity("view", by.ID, fmt.Sprintf("%s odayı %s moduna aldı", by.Name, names[v]))
	}
	r.broadcast(enc(msg))
}

// switchMedia makes md the room media; by nil = the queue advanced on its own.
func (r *Room) switchMedia(md *Media, by *Member, cid string) {
	r.cancelTimer()
	r.dl = nil
	if md.Kind == "download" {
		md.Task = r.newDownload()
	}
	r.media = md
	r.started = false
	r.waiting = nil
	r.undetachAll()
	r.mode = ModePaused
	r.anchorPos, r.anchorTs, r.startAt = 0, r.nowMs(), 0
	r.publish("media", by, cid)
	switch {
	case md.Kind == "page" && r.view != ViewBrowse:
		r.setView(ViewBrowse, nil, "") // tarayıcıda senkron: herkes Keşfet'te aynı sayfada
	case md.Kind != "page" && r.view == ViewBrowse && by != nil:
		r.setView(ViewWatch, nil, "") // film seçildi: sinemaya geç
	}
	switch {
	case by == nil:
		r.activity("media", "", "Sıradaki: "+md.Title)
	case md.Kind == "download":
		r.activity("media", by.ID, fmt.Sprintf("%s herkes için indirme başlattı: %s", by.Name, md.Title))
	default:
		r.activity("media", by.ID, fmt.Sprintf("%s açtı: %s", by.Name, md.Title))
	}
}

func cleanMedia(in *Media) (*Media, bool) {
	if in == nil {
		return nil, false
	}
	src := strings.TrimSpace(in.Src)
	if src == "" || len(src) > maxMediaSrc {
		return nil, false
	}
	referer := ""
	var file string
	var idx *int
	switch in.Kind {
	case "url", "download", "page":
		if !httpURL(src) {
			return nil, false
		}
		if r := strings.TrimSpace(in.Referer); in.Kind == "url" && len(r) <= maxMediaSrc && httpURL(r) {
			referer = r // optional: an invalid referer is dropped, not refused
		}
	case "file":
		if strings.ContainsAny(src, `/\`) {
			return nil, false // name only, never a local path
		}
	case "debrid":
		if !infoHash(src) {
			return nil, false
		}
		src = strings.ToLower(src)
		file = cleanTorrentFile(in.File)
		if in.Idx != nil && *in.Idx >= 0 && *in.Idx <= maxMediaIdx {
			v := *in.Idx // optional: an invalid index is dropped, not refused
			idx = &v
		}
	default:
		return nil, false
	}
	title := strings.TrimSpace(in.Title)
	if title == "" {
		title = src
	}
	if r := []rune(title); len(r) > maxMediaTitle {
		title = string(r[:maxMediaTitle])
	}
	dur := in.Duration
	if math.IsNaN(dur) || math.IsInf(dur, 0) || dur < 0 {
		dur = 0
	}
	return &Media{Kind: in.Kind, Src: src, Title: title, Duration: dur, Referer: referer, Subs: cleanSubs(in.Subs, referer), File: file, Idx: idx}, true
}

// infoHash reports whether s is a hex BitTorrent v1 info hash.
func infoHash(s string) bool {
	if len(s) != 40 {
		return false
	}
	for _, c := range s {
		if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f' || c >= 'A' && c <= 'F') {
			return false
		}
	}
	return true
}

// cleanTorrentFile keeps the base name of a file inside a torrent (a hint for picking
// the file, never a path); too long names are dropped.
func cleanTorrentFile(s string) string {
	s = strings.TrimSpace(s)
	if i := strings.LastIndexAny(s, `/\`); i >= 0 {
		s = s[i+1:]
	}
	if len([]rune(s)) > maxMediaFile {
		return ""
	}
	return s
}

const (
	maxSubs     = 16
	maxSubLang  = 16
	maxSubTitle = 100
)

// cleanSubs keeps valid subtitle entries of a captured stream; invalid ones are dropped.
func cleanSubs(in []Sub, referer string) []Sub {
	if referer == "" {
		return nil
	}
	var out []Sub
	for _, s := range in {
		u := strings.TrimSpace(s.URL)
		if len(out) == maxSubs || len(u) > maxMediaSrc || !httpURL(u) {
			continue
		}
		lang := strings.TrimSpace(s.Lang)
		if len(lang) > maxSubLang {
			lang = ""
		}
		title := strings.TrimSpace(s.Title)
		if r := []rune(title); len(r) > maxSubTitle {
			title = string(r[:maxSubTitle])
		}
		out = append(out, Sub{URL: u, Lang: lang, Title: title})
	}
	return out
}

func httpURL(s string) bool {
	u, err := url.Parse(s)
	return err == nil && (u.Scheme == "http" || u.Scheme == "https") && u.Host != ""
}

func (r *Room) playbackCmd(m *Member, in Inbound) {
	ended := r.media != nil && r.queue.atEnd == r.media
	r.queue.atEnd = nil // playing on (or seeking back) after the end: no auto-start
	now := r.nowMs()
	pos := r.position(now)
	explicit := in.Pos != nil && !math.IsNaN(*in.Pos) && *in.Pos >= 0
	if explicit {
		pos = *in.Pos
	}
	switch in.Op {
	case "play":
		if r.mode == ModePlaying || r.mode == ModeCountdown {
			// idempotent: tell the sender the current truth
			r.sendTo(m, enc(r.stateMsg("snapshot", nil, in.Cid)))
			return
		}
		r.waiting = nil
		if r.dl != nil {
			r.dl.started = true // "start with whoever is ready"
		}
		if ended && !explicit {
			// play after the end replays from the start; "seek" lets players jump back
			r.startCountdown(0, "seek", m, in.Cid)
			r.activity("play", m.ID, fmt.Sprintf("%s baştan başlattı", m.Name))
			return
		}
		r.startCountdown(pos, "play", m, in.Cid)
		r.activity("play", m.ID, fmt.Sprintf("%s başlattı: %s", m.Name, fmtPos(pos)))
	case "pause":
		r.cancelTimer()
		r.waiting = nil
		r.mode = ModePaused
		r.anchorPos = pos
		r.anchorTs = now
		r.startAt = 0
		r.publish("pause", m, in.Cid)
		r.activity("pause", m.ID, fmt.Sprintf("%s durdurdu: %s", m.Name, fmtPos(pos)))
	case "seek":
		if in.Pos == nil {
			r.deny(m, in.Cid, "bad_op")
			return
		}
		r.waiting = nil
		if r.dl != nil {
			r.dl.started = true
		}
		r.startCountdown(pos, "seek", m, in.Cid)
		r.activity("seek", m.ID, fmt.Sprintf("%s ileri/geri sardı: %s", m.Name, fmtPos(pos)))
	}
}

func (r *Room) handleReport(m *Member, in Inbound) {
	now := r.nowMs()
	pos := 0.0
	if in.Pos != nil {
		pos = *in.Pos
	}
	if in.SkippedSec > 0 {
		r.activity("skip", m.ID, fmt.Sprintf("%s %.0f s ileri atladı", m.Name, in.SkippedSec))
	}
	state := in.State
	if m.detached && (state == "stalled" || state == "ready") {
		state = "" // the room does not wait for a detached member
	}
	if !r.dlReady(m) && (state == "stalled" || state == "ready") {
		state = "" // still downloading: nothing loaded yet, nothing to wait for
	}
	switch state {
	case "stalled":
		switch r.mode {
		case ModePlaying, ModeCountdown:
			hp := r.position(now)
			r.cancelTimer()
			r.mode = ModeHold
			r.anchorPos, r.anchorTs, r.startAt = hp, now, 0
			r.waiting = []string{m.ID}
			r.publish("stall", nil, "")
			r.noteHold(m, hp)
		case ModeHold:
			if !r.isWaiting(m.ID) {
				r.waiting = append(r.waiting, m.ID)
				r.publish("stall", nil, "")
				r.noteHold(m, r.anchorPos)
			}
		}
	case "ready":
		if r.mode == ModeHold && r.dropWaiting(m.ID) {
			r.broadcast(enc(r.bufferMsg(m, in.BufferSec)))
			r.afterWaitingChanged(nil)
		}
	}
	if in.State == "refilling" || (in.State == "stalled" && r.mode == ModeHold) {
		if r.isWaiting(m.ID) {
			r.broadcast(enc(r.bufferMsg(m, in.BufferSec)))
		}
	}

	if now := r.cfg.Now(); now.Sub(m.lastStatus) >= statusInterval {
		m.lastStatus = now
		drift := 0
		if r.mode == ModePlaying {
			drift = int(math.Round((pos - r.position(now.UnixMilli())) * 1000))
		}
		st := map[string]any{
			"t": "member_status", "id": m.ID, "pos": pos, "driftMs": drift, "state": in.State,
			"bufferSec": in.BufferSec, "bufferPct": bufferPct(in.BufferSec),
		}
		if m.rtt > 0 {
			st["rttMs"] = m.rtt.Milliseconds()
		}
		r.broadcastExcept(m.ID, enc(st))
	}
}

func bufferPct(sec float64) int {
	p := sec / BufferTargetSec * 100
	if p > 100 {
		p = 100
	}
	if p < 0 {
		p = 0
	}
	return int(math.Round(p))
}

// Quick reactions (PROTOCOL.md §3.13): fixed emoji set, at most one per member per
// reactGap; anything else is dropped silently. Not in the feed, not recorded.
const reactGap = 700 * time.Millisecond

var reactEmoji = map[string]bool{"😂": true, "😱": true, "😍": true, "👏": true, "😭": true, "🔥": true}

func (r *Room) handleReact(m *Member, in Inbound) {
	now := r.cfg.Now()
	if !reactEmoji[in.Emoji] || now.Sub(m.lastReact) < reactGap {
		return
	}
	m.lastReact = now
	r.broadcast(enc(map[string]any{"t": "reaction", "user": m.ID, "name": m.Name, "emoji": in.Emoji}))
}

func (r *Room) bufferMsg(m *Member, sec float64) map[string]any {
	return map[string]any{"t": "buffer_status", "user": m.ID, "name": m.Name, "bufferPct": bufferPct(sec), "targetSec": int(BufferTargetSec)}
}

// ---- outbound ---------------------------------------------------------

// activity goes to every member's feed and, as the same kind, into the events log.
func (r *Room) activity(kind, user, text string) {
	now := r.feed(kind, user, text)
	r.record(kind, r.members[user], map[string]any{"text": text, "pos": math.Round(r.position(now)*1000) / 1000})
}

// feed broadcasts an activity line without recording it.
func (r *Room) feed(kind, user, text string) int64 {
	now := r.nowMs()
	r.broadcast(enc(map[string]any{"t": "activity", "kind": kind, "user": user, "text": text, "serverTs": now}))
	return now
}

// record appends to the events log (docs/design/accounts.md §5.3); m nil = system event.
func (r *Room) record(typ string, m *Member, payload any) {
	if r.cfg.Store == nil {
		return
	}
	var user int64
	if m != nil {
		user = m.User
	}
	r.recordUser(typ, user, payload)
}

// recordUser records for an account that may no longer be in the room.
func (r *Room) recordUser(typ string, user int64, payload any) {
	if r.cfg.Store == nil {
		return
	}
	r.cfg.Store.Record(r.id, user, typ, payload)
}

// closedNow: the room has shut down (safe from any goroutine).
func (r *Room) closedNow() bool {
	select {
	case <-r.done:
		return true
	default:
		return false
	}
}

func (r *Room) sendTo(m *Member, b []byte) {
	if m.closed || m.ghost {
		return
	}
	select {
	case m.send <- b:
	default:
		// slow consumer: drop the connection; the member may resume
		out := m.send
		go func() { r.do(func() { r.disconnect(m, out) }) }()
	}
}

func (r *Room) broadcast(b []byte) {
	for _, m := range r.members {
		r.sendTo(m, b)
	}
}

func (r *Room) broadcastExcept(id string, b []byte) {
	for _, m := range r.members {
		if m.ID != id {
			r.sendTo(m, b)
		}
	}
}
