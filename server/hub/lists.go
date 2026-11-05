package hub

import (
	"math"
	"strings"

	"github.com/serkanqltun-hub/koni/server/store"
)

// wantItem is a wanted title with the mutual friends who want it too.
type wantItem struct {
	store.Entry
	Mates []store.Holder `json:"mates,omitempty"`
}

func sameName(a, b string) bool { return strings.EqualFold(a, b) }

// markSeen: a movie the room watched past 80% goes onto everyone's "İzlediklerim"
// with who they watched it with. Movies only: a series isn't seen after one episode.
func (r *Room) markSeen() {
	md := r.media
	if r.cfg.Store == nil || md == nil || md.Kind != "debrid" || md.Duration <= 0 || r.seenDone[md] {
		return
	}
	if r.position(r.nowMs()) < 0.8*md.Duration {
		return
	}
	meta, _, _ := strings.Cut(md.File, "|")
	kind, id, _ := strings.Cut(meta, ":")
	if kind != "movie" || !strings.HasPrefix(id, "tt") || strings.Contains(id, ":") {
		return
	}
	r.seenDone[md] = true
	type who struct {
		user int64
		name string
	}
	var in []who
	for _, m := range r.members {
		if m.User != 0 && !m.ghost && !m.closed {
			in = append(in, who{m.User, m.Name})
		}
	}
	st, title := r.cfg.Store, md.Title
	go func() {
		for _, w := range in {
			var mates []string
			for _, o := range in {
				if o.user != w.user {
					mates = append(mates, o.name)
				}
			}
			_ = st.AddToList(w.user, store.ListSeen, store.Entry{Ref: id, Kind: "movie", Title: title, With: mates})
		}
	}()
}

// Lists answers a `lists` handshake (p0.23): "İzleyeceklerim" / "İzlediklerim" on
// profiles. Reading a friend's lists needs a mutual friendship.
func (h *Hub) Lists(in Inbound) map[string]any {
	st := h.cfg.Store
	if st == nil {
		return errMsg("no_accounts")
	}
	if in.Auth == "" {
		return errMsg("auth_required")
	}
	u, err := st.UserByToken(in.Auth)
	if err != nil {
		return errMsg(errCode(err))
	}
	switch in.Op {
	case "add":
		if in.Entry == nil {
			return errMsg("bad_entry")
		}
		err = st.AddToList(u.ID, in.List, *in.Entry)
	case "remove":
		if in.List == "progress" {
			err = st.RemoveProgress(u.ID, in.Ref)
		} else {
			err = st.RemoveFromList(u.ID, in.List, in.Ref)
		}
	case "rate":
		if in.Stars != 0 || in.Word != "" { // p0.25 stars; the ❤️/👎 rating stays
			err = st.RateStars(u.ID, in.Ref, in.Stars, in.Word)
		} else {
			err = st.RateSeen(u.ID, in.Ref, in.Rating)
		}
	case "progress":
		var pos float64
		if in.Pos != nil {
			pos = *in.Pos
		}
		err = st.SetProgress(u.ID, store.Progress{Ref: in.Ref, Kind: in.Kind, Season: in.Season, Episode: in.Episode,
			Pos: pos, Duration: in.Duration, Title: in.Title, Poster: in.Poster})
	case "seen_ep", "unseen_ep":
		err = st.SeenEpisode(u.ID, in.Ref, in.Season, in.Episode, in.Op == "seen_ep")
	case "who":
		want, seen, err := st.Who(u.ID, in.Ref)
		if err != nil {
			return errMsg("server_error")
		}
		mine := map[string]any{}
		var sum, n int
		for _, h := range seen {
			if h.Stars > 0 {
				sum, n = sum+h.Stars, n+1
			}
		}
		for _, l := range []string{store.ListWant, store.ListSeen} {
			es, err := st.List(u.ID, l)
			if err != nil {
				return errMsg("server_error")
			}
			for _, e := range es {
				if e.Ref == in.Ref {
					mine[l] = true
					if l == store.ListSeen {
						mine["rating"] = e.Rating
						if e.Stars > 0 {
							mine["stars"], mine["word"] = e.Stars, e.Word
							sum, n = sum+e.Stars, n+1
						}
					}
				}
			}
		}
		eps, err := st.Episodes(u.ID)
		if err != nil {
			return errMsg("server_error")
		}
		if e := eps[in.Ref]; len(e) > 0 {
			mine["episodes"] = e
		}
		out := map[string]any{"t": "lists", "op": "who", "ref": in.Ref, "want": want, "seen": seen, "mine": mine}
		if n > 0 { // you and your mutual friends who rated it (p0.25)
			out["stars"], out["starsCount"] = math.Round(float64(sum)/float64(n)*10)/10, n
		}
		return out
	case "":
	default:
		return errMsg("bad_message")
	}
	if err != nil {
		return errMsg(errCode(err))
	}

	who := u
	if in.Op == "" && in.User != "" {
		f, err := st.Lookup(in.User)
		if err != nil {
			return errMsg(errCode(err))
		}
		if f.ID != u.ID {
			if ok, _ := st.Mutual(u.ID, f.ID); !ok {
				return errMsg("not_friends")
			}
		}
		who = f
	}
	want, err := st.List(who.ID, store.ListWant)
	if err != nil {
		return errMsg("server_error")
	}
	seen, err := st.List(who.ID, store.ListSeen)
	if err != nil {
		return errMsg("server_error")
	}
	out := map[string]any{"t": "lists", "user": who.Handle(), "avatar": who.Avatar, "seen": seen}
	if who.ID == u.ID {
		mates, err := st.WantMates(u.ID)
		if err != nil {
			return errMsg("server_error")
		}
		items := make([]wantItem, len(want))
		for i, e := range want {
			items[i] = wantItem{Entry: e, Mates: mates[e.Ref]}
		}
		out["want"] = items
		prog, err := st.ProgressList(u.ID)
		if err != nil {
			return errMsg("server_error")
		}
		eps, err := st.Episodes(u.ID)
		if err != nil {
			return errMsg("server_error")
		}
		out["progress"], out["episodes"] = prog, eps
		return out
	}
	// A friend's profile: their lists, and what both of you want ("ortak liste").
	mine, err := st.List(u.ID, store.ListWant)
	if err != nil {
		return errMsg("server_error")
	}
	have := map[string]bool{}
	for _, e := range mine {
		have[e.Ref] = true
	}
	common := []string{}
	for _, e := range want {
		if have[e.Ref] {
			common = append(common, e.Ref)
		}
	}
	together := 0
	for _, e := range seen {
		for _, w := range e.With {
			if sameName(w, u.Name) {
				together++
				break
			}
		}
	}
	out["want"], out["common"], out["together"] = want, common, together
	return out
}
