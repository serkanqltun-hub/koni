package hub

import (
	"encoding/json"

	"github.com/serkanqltun-hub/koni/server/discover"
	"github.com/serkanqltun-hub/koni/server/store"
)

// Protocol version, see docs/PROTOCOL.md.
const ProtocolVersion = "p0.24"

// Inbound is the union of all client→server messages; fields are used per type "t".
type Inbound struct {
	T string `json:"t"`

	// create_room / join (Auth: account session token, required when the server has accounts)
	Name     string `json:"name,omitempty"`
	Username string `json:"username,omitempty"`
	Policy   string `json:"policy,omitempty"`
	// cmd set_view: "browse" | "watch" | "listen"
	View   string `json:"view,omitempty"`
	Invite string `json:"invite,omitempty"`
	Auth   string `json:"auth,omitempty"`

	// register (Code: one-time account invite) / login / logout (Auth)
	Code     string `json:"code,omitempty"`
	Password string `json:"password,omitempty"`
	Device   string `json:"device,omitempty"`
	// register / profile op "avatar": one of the server's picture ids (p0.18)
	Avatar string `json:"avatar,omitempty"`
	// profile op "upload" (admins, p0.19): base64 JPEG/PNG/WebP, at most 180 KiB
	Image string `json:"image,omitempty"`

	// stats (Auth required): "all", "month" or "week"
	// friends (Auth required): Op "list" | "add" | "remove" | "decline", User = account name
	Period string `json:"period,omitempty"`

	// resume
	Room   string `json:"room,omitempty"`
	Member string `json:"member,omitempty"`
	Token  string `json:"token,omitempty"`

	// cmd
	Cid   string   `json:"cid,omitempty"`
	Op    string   `json:"op,omitempty"`
	Pos   *float64 `json:"pos,omitempty"`
	Media *Media   `json:"media,omitempty"` // op "media"
	User  string   `json:"user,omitempty"`  // ops "continue_without", "set_cohost"
	On    bool     `json:"on,omitempty"`    // op "set_cohost"

	// report (State is also used by dl_progress)
	State      string  `json:"state,omitempty"`
	BufferSec  float64 `json:"bufferSec,omitempty"`
	SkippedSec float64 `json:"skippedSec,omitempty"`

	// dl_progress
	TaskID string  `json:"taskId,omitempty"`
	Pct    float64 `json:"pct,omitempty"`
	Speed  float64 `json:"speed,omitempty"`
	Eta    float64 `json:"eta,omitempty"`
	Error  string  `json:"error,omitempty"`

	// react (§3.13)
	Emoji string `json:"emoji,omitempty"`

	// bug_report (§3.0b; Auth on an account server, Username on an open one)
	Version string `json:"version,omitempty"`
	Text    string `json:"text,omitempty"`

	// ping
	T0    int64 `json:"t0,omitempty"`
	RttMs int64 `json:"rtt,omitempty"` // client's last measured RTT

	// discover / swipe / propose / unpropose (docs/PROTOCOL.md §3.9)
	Mood       string `json:"mood,omitempty"`
	MaxRuntime int    `json:"maxRuntime,omitempty"` // minutes
	DeckID     string `json:"deckId,omitempty"`
	ItemID     int    `json:"itemId,omitempty"` // TMDB id
	Vote       string `json:"vote,omitempty"`   // like | pass
	Query      string `json:"query,omitempty"`  // propose: typed title; subs_search: name
	ProposalID string `json:"proposalId,omitempty"`

	// night_* (§3.9a, p0.17): Kind/PerUser for night_start, Ready for night_ready and
	// night_done (Mood carries the label), Filter for night_suggest, Filters for
	// night_preview; night_search uses Query, night_detail/pick/unpick ItemID.
	Kind    string            `json:"kind,omitempty"`
	PerUser int               `json:"perUser,omitempty"`
	Ready   bool              `json:"ready,omitempty"`
	Filter  *discover.Filter  `json:"filter,omitempty"`
	Filters []discover.Tagged `json:"filters,omitempty"`

	// lists (p0.23): Op "" (read; User = a mutual friend's Ad#1234, empty = you) |
	// "add" (List, Entry) | "remove" (List, Ref) | "rate" (Ref, Rating) | "who" (Ref)
	List   string       `json:"list,omitempty"`
	Ref    string       `json:"ref,omitempty"`
	Rating int          `json:"rating,omitempty"`
	Entry  *store.Entry `json:"entry,omitempty"`

	// subs_search / subs_pick (§3.10)
	Hash   string `json:"hash,omitempty"` // OpenSubtitles moviehash of the local file
	IMDb   string `json:"imdb,omitempty"`
	Lang   string `json:"lang,omitempty"` // "tr", "en" or "tr,en"
	FileID int    `json:"fileId,omitempty"`

	// queue_vote / queue_remove / track_end (§3.11); queue_add uses Media
	QueueID string `json:"queueId,omitempty"`
	// track_end: the member's player ended src; queue_next: the song the member saw
	// playing (optional; a skip for an already skipped song is ignored).
	Src string `json:"src,omitempty"`
	// track_end: src could not be opened on this member's player.
	Failed bool `json:"failed,omitempty"`
}

type MemberInfo struct {
	ID       string `json:"id"`
	Name     string `json:"name"`
	Host     bool   `json:"host"`
	CoHost   bool   `json:"cohost,omitempty"`   // may control playback in host_only, gets detach offers
	Admin    bool   `json:"admin,omitempty"`    // server admin (p0.19): the frame
	Away     bool   `json:"away,omitempty"`     // connection lost, may resume
	Detached bool   `json:"detached,omitempty"` // room continues without them
}

type ByInfo struct {
	ID   string `json:"id"`
	Name string `json:"name"`
}

// Media is what the room is watching. Kind "url": every client resolves Src itself
// (yt-dlp stream URLs are bound to the resolving client's IP). Kind "file": Src is a
// file name; every member opens their own copy. Kind "download": Src is a web page
// every member downloads before playback starts together; Task names the barrier.
// Referer (kind "url" only): page a captured stream came from; clients send it as
// the HTTP Referer when loading Src. Subs (kind "url" with Referer only): subtitle
// files the capture saw on the page; every client adds them to the player.
// Kind "debrid" (p0.15): Src is a torrent info hash (40 hex); every member resolves
// it with their own Real-Debrid account. File (file name in
// the torrent) and Idx (Stremio fileIdx) pick the file; no account data is shared.
// Kind "page" (p0.20): Src is a web page; every member opens it in Keşfet's browser
// and the page's own <video> follows the room clock (tarayıcıda senkron). The room
// switches to the browse view.
type Media struct {
	Kind     string  `json:"kind"`
	Src      string  `json:"src"`
	Title    string  `json:"title,omitempty"`
	Duration float64 `json:"duration,omitempty"`
	Task     string  `json:"task,omitempty"`
	Referer  string  `json:"referer,omitempty"`
	Subs     []Sub   `json:"subs,omitempty"`
	File     string  `json:"file,omitempty"`
	Idx      *int    `json:"idx,omitempty"`
}

// Sub is a subtitle file of a captured stream.
type Sub struct {
	URL   string `json:"url"`
	Lang  string `json:"lang,omitempty"`
	Title string `json:"title,omitempty"`
}

// DlStatus is one member's progress on the room's download task.
type DlStatus struct {
	User  string  `json:"user"`
	Name  string  `json:"name"`
	State string  `json:"state"` // queued | downloading | ready | failed
	Pct   float64 `json:"pct"`
	Speed float64 `json:"speed,omitempty"` // bytes/s
	Eta   float64 `json:"eta,omitempty"`   // s
	Error string  `json:"error,omitempty"`
}

// StateMsg is the room anchor broadcast (type "state").
type StateMsg struct {
	T          string   `json:"t"`
	Seq        int64    `json:"seq"`
	Mode       string   `json:"mode"`
	AnchorPos  float64  `json:"anchorPos"`
	AnchorTs   int64    `json:"anchorTs"`
	StartAt    int64    `json:"startAt"`
	Rate       float64  `json:"rate"`
	By         *ByInfo  `json:"by,omitempty"`
	Cause      string   `json:"cause"`
	Ack        string   `json:"ack,omitempty"`
	Reason     string   `json:"reason,omitempty"`
	HoldPos    *float64 `json:"holdPos,omitempty"`
	WaitingFor []string `json:"waitingFor,omitempty"`
	Media      *Media   `json:"media,omitempty"`
	ServerTs   int64    `json:"serverTs"`
}

type Snapshot struct {
	Room      string       `json:"room"`
	Name      string       `json:"name"`
	Policy    string       `json:"policy"`
	View      string       `json:"view"`
	Host      string       `json:"host"`
	Members   []MemberInfo `json:"members"`
	State     StateMsg     `json:"state"`
	Downloads []DlStatus   `json:"downloads,omitempty"` // current task, join order
}

func enc(v any) []byte {
	b, _ := json.Marshal(v)
	return b
}
