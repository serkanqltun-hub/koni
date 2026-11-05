package hub

import (
	"context"
	"strconv"
	"strings"

	"github.com/serkanqltun-hub/koni/server/discover"
	"github.com/serkanqltun-hub/koni/server/store"
)

// maxWatchSec caps one solo watch report: the client reports about once a minute.
const maxWatchSec = 120

// Watch records solo watching (p0.25 `watch`): a `watch` event like a room's, with
// solo:true and room "solo:<account>". A report can't count more time than has passed
// since the account's previous one.
func (h *Hub) Watch(in Inbound) map[string]any {
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
	now := h.cfg.Now().UnixMilli()
	sec := int64(min(max(in.Sec, 0), maxWatchSec))
	h.watchMu.Lock()
	if last, ok := h.watchAt[u.ID]; ok {
		sec = min(sec, max(0, (now-last)/1000))
	}
	if sec > 0 {
		h.watchAt[u.ID] = now
	}
	h.watchMu.Unlock()
	if sec > 0 {
		ms := sec * 1000
		ev := map[string]any{"ms": ms, "night": store.NightMs(now-ms, now), "solo": true}
		if t := []rune(strings.TrimSpace(in.Title)); len(t) > 0 {
			ev["title"] = string(t[:min(len(t), 200)])
		}
		st.Record("solo:"+strconv.FormatInt(u.ID, 10), u.ID, "watch", ev)
	}
	return map[string]any{"t": "watch", "sec": sec}
}

// similarSize: how many "Benzerleri" a detail carries.
const similarSize = 10

// Detail is night_detail without a room (p0.25 `detail`): the full card for ref
// ("tt…" or "tmdb:<movie|tv>:<id>") plus similar titles.
func (h *Hub) Detail(ctx context.Context, in Inbound) map[string]any {
	if st := h.cfg.Store; st != nil {
		if in.Auth == "" {
			return errMsg("auth_required")
		}
		if _, err := st.UserByToken(in.Auth); err != nil {
			return errMsg(errCode(err))
		}
	}
	d := h.cfg.Discover
	if d == nil {
		return map[string]any{"t": "detail", "ref": in.Ref, "error": "Bu sunucuda künye bilgisi kapalı"}
	}
	ctx, cancel := context.WithTimeout(ctx, upstreamTimeout)
	defer cancel()
	kind, id, err := resolveRef(ctx, d, in.Ref, in.Kind)
	var it discover.Item
	if err == nil {
		it, err = d.Detail(ctx, kind, id)
	}
	if err != nil {
		return map[string]any{"t": "detail", "ref": in.Ref, "error": err.Error()}
	}
	similar, err := d.Similar(ctx, kind, id, similarSize)
	if err != nil {
		similar = []discover.Item{}
	}
	return map[string]any{"t": "detail", "ref": in.Ref, "item": it, "similar": similar}
}

// resolveRef turns a list ref into a TMDB kind and id. kind ("movie", "series"/"tv")
// helps an IMDb id; without it a movie is tried first.
func resolveRef(ctx context.Context, d *discover.Client, ref, kind string) (string, int, error) {
	if kind == "series" {
		kind = discover.KindTV
	}
	if rest, ok := strings.CutPrefix(ref, "tmdb:"); ok {
		k, n, _ := strings.Cut(rest, ":")
		if k == "series" {
			k = discover.KindTV
		}
		id, err := strconv.Atoi(n)
		if !discover.ValidKind(k) || err != nil || id <= 0 {
			return "", 0, discover.ErrNotFound
		}
		return k, id, nil
	}
	if !strings.HasPrefix(ref, "tt") || len(ref) > 16 {
		return "", 0, discover.ErrNotFound
	}
	if discover.ValidKind(kind) {
		id, err := d.FindIMDb(ctx, kind, ref)
		return kind, id, err
	}
	for _, k := range []string{discover.KindMovie, discover.KindTV} {
		if id, err := d.FindIMDb(ctx, k, ref); err == nil {
			return k, id, nil
		}
	}
	return "", 0, discover.ErrNotFound
}
