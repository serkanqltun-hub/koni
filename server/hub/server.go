package hub

import (
	"context"
	"encoding/json"
	"net/http"
	"time"

	"github.com/coder/websocket"
)

const (
	maxFrame      = 8 << 10
	handshakeWait = 5 * time.Second
	writeWait     = 5 * time.Second
	readIdle      = 20 * time.Second // clients ping every 3 s; silence means a dead link
)

// Handler returns the HTTP mux: /ws, /healthz, /avatars and /auth. The server accepts no file uploads.
func (h *Hub) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("/healthz", func(w http.ResponseWriter, _ *http.Request) { w.Write([]byte("ok")) })
	mux.HandleFunc("/ws", h.serveWS)
	mux.HandleFunc("GET /avatars/{id}", h.serveAvatar)
	mux.HandleFunc("GET /auth", h.serveAuth)
	return mux
}

func writeJSON(ctx context.Context, c *websocket.Conn, v any) error {
	ctx, cancel := context.WithTimeout(ctx, writeWait)
	defer cancel()
	return c.Write(ctx, websocket.MessageText, enc(v))
}

func (h *Hub) serveWS(w http.ResponseWriter, req *http.Request) {
	// Desktop client (non-browser) → no Origin; allow any origin pattern.
	c, err := websocket.Accept(w, req, &websocket.AcceptOptions{InsecureSkipVerify: true})
	if err != nil {
		return
	}
	c.SetReadLimit(maxReportFrame) // only a bug_report handshake may be this big
	ctx := req.Context()
	defer c.CloseNow()

	// 1. handshake message
	hctx, cancel := context.WithTimeout(ctx, handshakeWait)
	_, data, err := c.Read(hctx)
	cancel()
	if err != nil {
		return
	}
	c.SetReadLimit(maxFrame)
	var in Inbound
	if json.Unmarshal(data, &in) != nil {
		writeJSON(ctx, c, map[string]any{"t": "error", "code": "bad_message", "message": "geçersiz JSON"})
		return
	}
	var (
		sess *Session
		code string
	)
	switch in.T {
	case "stats":
		// one-shot like the account handshake: reply, then close
		writeJSON(ctx, c, h.Stats(in))
		c.Close(websocket.StatusNormalClosure, "")
		return
	case "friends":
		writeJSON(ctx, c, h.Friends(in))
		h.pokePresence()
		c.Close(websocket.StatusNormalClosure, "")
		return
	case "lists":
		writeJSON(ctx, c, h.Lists(in))
		c.Close(websocket.StatusNormalClosure, "")
		return
	case "feed":
		writeJSON(ctx, c, h.Feed(in))
		c.Close(websocket.StatusNormalClosure, "")
		return
	case "profile":
		writeJSON(ctx, c, h.Profile(in))
		c.Close(websocket.StatusNormalClosure, "")
		return
	case "admin":
		writeJSON(ctx, c, h.Admin(in))
		c.Close(websocket.StatusNormalClosure, "")
		return
	case "hello":
		// lobby: stays open while the app runs (presence, calls)
		h.Lobby(ctx, c, in)
		c.Close(websocket.StatusNormalClosure, "")
		return
	case "bug_report":
		writeJSON(ctx, c, h.BugReport(in, clientIP(req)))
		c.Close(websocket.StatusNormalClosure, "")
		return
	case "register", "login", "logout":
		// account handshake: one reply, then the connection ends
		writeJSON(ctx, c, h.Auth(in, clientIP(req)))
		c.Close(websocket.StatusNormalClosure, "")
		return
	case "create_room":
		sess, code = h.CreateRoom(in.Name, in.Username, in.Policy, in.Auth)
	case "join":
		sess, code = h.JoinRoom(in.Invite, in.Username, in.Auth)
	case "resume":
		sess, code = h.Resume(in.Room, in.Member, in.Token)
	default:
		code = "not_joined"
	}
	if sess == nil {
		writeJSON(ctx, c, map[string]any{"t": "error", "code": code, "message": errText[code]})
		c.Close(websocket.StatusPolicyViolation, code)
		return
	}
	// friends' lobbies see the room change now, not on the next tick
	h.pokePresence()
	defer h.pokePresence()
	defer sess.Close()

	if in.T == "create_room" {
		writeJSON(ctx, c, map[string]any{"t": "room_created", "room": sess.RoomID, "name": sess.Snap.Name, "invite": sess.Invite, "you": sess.Member.ID})
	}
	writeJSON(ctx, c, map[string]any{"t": "joined", "you": sess.Member.ID, "invite": sess.Invite, "snapshot": sess.Snap, "serverTs": time.Now().UnixMilli(), "protocol": ProtocolVersion, "resume": sess.Token, "resumed": sess.Resumed})

	// 2. writer
	wctx, wcancel := context.WithCancel(ctx)
	defer wcancel()
	go func() {
		defer wcancel()
		for b := range sess.Out() {
			cx, cl := context.WithTimeout(wctx, writeWait)
			err := c.Write(cx, websocket.MessageText, b)
			cl()
			if err != nil {
				return
			}
		}
		c.Close(websocket.StatusPolicyViolation, "removed")
	}()

	// 3. reader
	for {
		rctx, rcancel := context.WithTimeout(wctx, readIdle)
		_, data, err := c.Read(rctx)
		rcancel()
		if err != nil {
			return
		}
		var m Inbound
		if json.Unmarshal(data, &m) != nil {
			sess.Handle(Inbound{T: "?"}) // yields bad_message to this member
			continue
		}
		if m.T == "create_room" || m.T == "join" || m.T == "resume" || m.T == "register" || m.T == "login" || m.T == "logout" || m.T == "friends" || m.T == "feed" || m.T == "lists" || m.T == "profile" || m.T == "hello" || m.T == "admin" {
			m.T = "?"
		}
		if m.T == "call" { // ask a mutual friend in (p0.18): answered to this member only
			reply := enc(h.Call(sess, m.User))
			sess.Room.do(func() { sess.Room.sendTo(sess.Member, reply) })
			continue
		}
		sess.Handle(m)
		if m.T == "leave" {
			return
		}
		if m.T == "close_room" { // a host's close ends this connection too
			gone := true
			sess.Room.call(func() { gone = sess.Member.closed })
			if gone {
				return
			}
		}
	}
}

var errText = map[string]string{
	"bad_message":      "Geçersiz istek",
	"bad_invite":       "Davet geçersiz",
	"bad_username":     "Kullanıcı adı 1–24 karakter olmalı",
	"name_taken":       "Bu kullanıcı adı odada kullanılıyor",
	"not_joined":       "Önce create_room veya join gönderin",
	"room_full":        "Oda dolu",
	"resume_failed":    "Oturum sürdürülemedi",
	"auth_required":    "Bu sunucu hesap istiyor: giriş yap",
	"bad_token":        "Oturumun geçersiz, yeniden giriş yap",
	"bad_code":         "Davet kodu geçersiz, kullanılmış ya da süresi dolmuş",
	"bad_password":     "Şifre 8–128 karakter olmalı",
	"username_taken":   "Bu kullanıcı adı alınmış",
	"bad_login":        "Kullanıcı adı ya da şifre yanlış",
	"rate_limited":     "Çok fazla deneme, biraz sonra tekrar dene",
	"no_accounts":      "Bu sunucuda hesap yok, kullanıcı adıyla gir",
	"server_error":     "Sunucu hatası",
	"bad_period":       "Dönem all, month ya da week olmalı",
	"too_large":        "Rapor çok büyük (en fazla 200 KB)",
	"no_reports":       "Bu sunucu hata bildirimi almıyor",
	"unknown_user":     "Bu adla bir hesap yok",
	"self_friend":      "Kendini arkadaş ekleyemezsin",
	"bad_avatar":       "Bu resim sunucuda yok",
	"too_many_invites": "En fazla 5 açık davet kodun olabilir",
	"not_friends":      "Yalnızca karşılıklı arkadaşlarını çağırabilirsin",
	"not_invited":      "Bu odaya çağrılmadın: odadaki bir arkadaşın seni çağırmalı",
	"forbidden":        "Bunu yalnızca adminler yapabilir",
	"avatar_taken":     "Bu fotoğrafı bir admin kullanıyor",
	"room_closed":      "Oda bir admin tarafından kapatıldı",
	"room_ended":       "Host odayı herkes için kapattı",
	"bad_entry":        "Başlık eksik",
	"banned":           "Hesabın bir admin tarafından silindi",
	"no_invites":       "Davet hakkın bir admin tarafından alındı",
}
