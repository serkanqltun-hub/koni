package hub

import (
	"encoding/base64"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"time"

	"github.com/serkanqltun-hub/koni/server/store"
)

type storeMember = store.Member

// ---------------------------------------------------------------- admins (p0.19)
//
// Admins see every room and member, close rooms, ban members (the account is deleted;
// a non-admin inviter loses the invite right) and may use their own profile picture.
// Admins are made on the server: `koni-server admin -db … Ad#1234`.

const maxAdminAvatar = 180 << 10

// uploadAvatar adds an admin's picture (base64 JPEG/PNG/WebP) to the shared picker
// and returns its id. While an admin wears a picture nobody else can pick it; when
// they change it, it is free for everyone.
func (h *Hub) uploadAvatar(b64 string) (string, string) {
	if h.cfg.Avatars == "" {
		return "", "bad_avatar"
	}
	data, err := base64.StdEncoding.DecodeString(b64)
	if err != nil || len(data) == 0 || len(data) > maxAdminAvatar {
		return "", "bad_avatar"
	}
	ext := map[string]string{"image/jpeg": ".jpg", "image/png": ".png", "image/webp": ".webp"}[http.DetectContentType(data)]
	if ext == "" {
		return "", "bad_avatar"
	}
	id := "ad-" + token(6)
	if err := os.WriteFile(filepath.Join(h.cfg.Avatars, id+ext), data, 0o644); err != nil {
		return "", "server_error"
	}
	return id, ""
}

// takenAvatars: pictures admins (other than user) are wearing.
func (h *Hub) takenAvatars(user int64) []string {
	ids, err := h.cfg.Store.AdminAvatars(user)
	if err != nil {
		return nil
	}
	return ids
}

type adminRoom struct {
	ID      string      `json:"id"`
	Name    string      `json:"name"`
	View    string      `json:"view"`
	Title   string      `json:"title,omitempty"`
	Members []adminSeat `json:"members"`
	Since   int64       `json:"since"`
}

type adminSeat struct {
	Name   string `json:"name"`
	Avatar string `json:"avatar,omitempty"`
	Host   bool   `json:"host,omitempty"`
	Admin  bool   `json:"admin,omitempty"`
}

func (h *Hub) adminRooms() []adminRoom {
	h.mu.Lock()
	rooms := make([]*Room, 0, len(h.byID))
	for _, r := range h.byID {
		rooms = append(rooms, r)
	}
	h.mu.Unlock()
	out := []adminRoom{}
	avatars := map[int64]string{}
	for _, r := range rooms {
		var ar adminRoom
		var users []int64
		ok := r.call(func() {
			ar = adminRoom{ID: r.id, Name: r.name, View: r.view, Since: r.created, Members: []adminSeat{}}
			if r.media != nil {
				ar.Title = r.media.Title
			}
			for _, id := range r.order {
				m := r.members[id]
				if m == nil || m.closed {
					continue
				}
				ar.Members = append(ar.Members, adminSeat{Name: m.Name, Host: m.Host, Admin: m.Admin})
				users = append(users, m.User)
			}
		})
		if !ok {
			continue
		}
		for i, u := range users {
			if u == 0 {
				continue
			}
			if _, seen := avatars[u]; !seen {
				if acc, err := h.cfg.Store.UserByID(u); err == nil {
					avatars[u] = acc.Avatar
				}
			}
			ar.Members[i].Avatar = avatars[u]
		}
		out = append(out, ar)
	}
	return out
}

// closeRoom sends everyone home and forgets the room.
func (h *Hub) closeRoom(id, why string) bool {
	h.mu.Lock()
	r := h.byID[id]
	h.mu.Unlock()
	if r == nil {
		return false
	}
	h.remove(r)
	r.do(func() {
		msg := enc(errMsg(why))
		for _, m := range r.members {
			if !m.ghost {
				r.sendTo(m, msg)
			}
			r.leave(m)
		}
	})
	return true
}

// kickUser removes an account from every room it is in.
func (h *Hub) kickUser(user int64) {
	h.mu.Lock()
	rooms := make([]*Room, 0, len(h.byID))
	for _, r := range h.byID {
		rooms = append(rooms, r)
	}
	h.mu.Unlock()
	msg := enc(errMsg("banned"))
	for _, r := range rooms {
		r.do(func() {
			for _, m := range r.members {
				if m.User == user {
					if !m.ghost {
						r.sendTo(m, msg)
					}
					r.leave(m)
				}
			}
		})
	}
	h.soc.mu.Lock()
	delete(h.soc.calls, user)
	h.soc.mu.Unlock()
}

// Admin answers an `admin` handshake: op "rooms" | "users" | "close" (Room) | "ban" (User)
// | "invite_right" (User, On). Every op replies with the current rooms and members.
func (h *Hub) Admin(in Inbound) map[string]any {
	st := h.cfg.Store
	if st == nil {
		return errMsg("no_accounts")
	}
	u, err := st.UserByToken(in.Auth)
	if err != nil {
		return errMsg(errCode(err))
	}
	if !u.Admin {
		return errMsg("forbidden")
	}
	switch in.Op {
	case "", "list", "rooms", "users":
	case "close":
		if !h.closeRoom(in.Room, "room_closed") {
			return errMsg("bad_invite")
		}
		log.Printf("admin %s closed room %s", u.Handle(), in.Room)
	case "ban":
		t, err := st.Lookup(in.User)
		if err != nil {
			return errMsg(errCode(err))
		}
		if t.ID == u.ID {
			return errMsg("forbidden")
		}
		if admin, _ := st.IsAdmin(t.ID); admin {
			return errMsg("forbidden")
		}
		inviter, err := st.Ban(t.ID)
		if err != nil {
			return errMsg(errCode(err))
		}
		h.kickUser(t.ID)
		h.pokePresence()
		log.Printf("admin %s banned %s (inviter %d loses invites: %v)", u.Handle(), t.Handle(), inviter, inviter != 0)
	case "invite_right":
		t, err := st.Lookup(in.User)
		if err != nil {
			return errMsg(errCode(err))
		}
		if err := st.SetInviteRight(t.ID, in.On); err != nil {
			return errMsg(errCode(err))
		}
	default:
		return errMsg("bad_message")
	}
	members, err := st.Members()
	if err != nil {
		return errMsg(errCode(err))
	}
	where, _ := h.whereabouts()
	h.soc.mu.Lock()
	type row struct {
		storeMember
		Online bool   `json:"online"`
		Room   string `json:"room,omitempty"`
	}
	rows := make([]row, 0, len(members))
	for _, m := range members {
		_, lobby := h.soc.conns[m.ID]
		w, inRoom := where[m.ID]
		rows = append(rows, row{storeMember: m, Online: lobby || inRoom, Room: w.Room})
	}
	h.soc.mu.Unlock()
	return map[string]any{"t": "admin", "rooms": h.adminRooms(), "members": rows, "now": time.Now().UnixMilli()}
}
