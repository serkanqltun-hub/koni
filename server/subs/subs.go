// Package subs is the Santral's OpenSubtitles proxy (docs/design/media.md §6.2): the
// API key (and optional account) stay on the server; clients send a moviehash or a
// title and get subtitle text back.
package subs

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
	"unicode/utf8"
)

const (
	MaxText    = 1 << 20 // larger files are not subtitles
	MaxResults = 20
	userAgent  = "KONI v0.9"
)

var (
	ErrNotFound = errors.New("altyazı bulunamadı")
	hashRe      = regexp.MustCompile(`^[0-9a-f]{16}$`)
	imdbRe      = regexp.MustCompile(`^(tt)?\d{1,9}$`)
	seriesRe    = regexp.MustCompile(`^(?:tt)?(\d{1,9}):(\d+):(\d+)$`)
	epTextRe    = regexp.MustCompile(`(?i)^(.*?)\s+s(\d{1,2})e(\d{1,2})$`)
)

// Result is one downloadable subtitle file.
type Result struct {
	FileID    int    `json:"fileId"`
	Name      string `json:"name"`
	Lang      string `json:"lang"`
	Downloads int    `json:"downloads"`
	HashMatch bool   `json:"hashMatch,omitempty"` // made for this exact file
	HI        bool   `json:"hi,omitempty"`        // hearing impaired
	Machine   bool   `json:"machine,omitempty"`   // machine/AI translated
}

// Query: Hash (OpenSubtitles moviehash of a local file), else IMDb id, else Text.
type Query struct {
	Hash  string
	IMDb  string
	Text  string
	Langs []string // "tr", "en"
}

type Client struct {
	key, user, pass string
	Base            string // overridable in tests
	HTTP            *http.Client

	mu       sync.Mutex
	token    string
	tokenExp time.Time
}

// New: user/pass are optional (an account raises the daily download quota).
func New(key, user, pass string) *Client {
	return &Client{
		key: strings.TrimSpace(key), user: strings.TrimSpace(user), pass: pass,
		Base: "https://api.opensubtitles.com/api/v1", HTTP: &http.Client{Timeout: 15 * time.Second},
	}
}

func (c *Client) do(ctx context.Context, method, path string, body any, out any) error {
	var rd io.Reader
	if body != nil {
		b, _ := json.Marshal(body)
		rd = bytes.NewReader(b)
	}
	req, err := http.NewRequestWithContext(ctx, method, c.Base+path, rd)
	if err != nil {
		return err
	}
	req.Header.Set("Api-Key", c.key)
	req.Header.Set("User-Agent", userAgent)
	req.Header.Set("Accept", "application/json")
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	if path != "/login" {
		if tok := c.session(ctx); tok != "" {
			req.Header.Set("Authorization", "Bearer "+tok)
		}
	}
	resp, err := c.HTTP.Do(req)
	if err != nil {
		return fmt.Errorf("OpenSubtitles: %w", err)
	}
	defer resp.Body.Close()
	b, err := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	if err != nil {
		return err
	}
	if resp.StatusCode != http.StatusOK {
		var e struct {
			Message string `json:"message"`
		}
		_ = json.Unmarshal(b, &e)
		switch {
		case resp.StatusCode == http.StatusUnauthorized && path == "/login":
			return errors.New("OpenSubtitles hesabı reddedildi")
		case resp.StatusCode == http.StatusUnauthorized || resp.StatusCode == http.StatusForbidden:
			return errors.New("OpenSubtitles anahtarı geçersiz")
		case resp.StatusCode == 406 || resp.StatusCode == http.StatusTooManyRequests:
			if e.Message != "" {
				return fmt.Errorf("OpenSubtitles kotası: %s", e.Message)
			}
			return errors.New("OpenSubtitles günlük indirme kotası doldu")
		case e.Message != "":
			return fmt.Errorf("OpenSubtitles: %s", e.Message)
		}
		return fmt.Errorf("OpenSubtitles: HTTP %d", resp.StatusCode)
	}
	return json.Unmarshal(b, out)
}

// session logs in once a day when an account is configured; "" = API key only.
func (c *Client) session(ctx context.Context) string {
	if c.user == "" {
		return ""
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.token != "" && time.Now().Before(c.tokenExp) {
		return c.token
	}
	var r struct {
		Token string `json:"token"`
	}
	if err := c.do(ctx, http.MethodPost, "/login", map[string]string{"username": c.user, "password": c.pass}, &r); err != nil {
		return ""
	}
	c.token, c.tokenExp = r.Token, time.Now().Add(23*time.Hour)
	return c.token
}

// Search lists subtitles, best first: exact file (hash) matches, human translations,
// then most downloaded.
func (c *Client) Search(ctx context.Context, q Query) ([]Result, error) {
	v := url.Values{}
	langs := make([]string, 0, len(q.Langs))
	for _, l := range q.Langs {
		if l = strings.ToLower(strings.TrimSpace(l)); len(l) == 2 {
			langs = append(langs, l)
		}
	}
	if len(langs) == 0 {
		langs = []string{"tr", "en"}
	}
	sort.Strings(langs)
	v.Set("languages", strings.Join(langs, ","))
	hash := strings.ToLower(q.Hash)
	switch {
	case hashRe.MatchString(hash):
		v.Set("moviehash", hash)
		if t := strings.ToLower(strings.TrimSpace(q.Text)); t != "" {
			v.Set("query", t) // falls back to the name when no file has this hash
		}
	case seriesRe.MatchString(q.IMDb):
		m := seriesRe.FindStringSubmatch(q.IMDb)
		n, _ := strconv.Atoi(m[1])
		s, _ := strconv.Atoi(m[2])
		e, _ := strconv.Atoi(m[3])
		v.Set("parent_imdb_id", strconv.Itoa(n))
		v.Set("season_number", strconv.Itoa(s))
		v.Set("episode_number", strconv.Itoa(e))
	case imdbRe.MatchString(q.IMDb):
		n, _ := strconv.Atoi(strings.TrimPrefix(q.IMDb, "tt"))
		v.Set("imdb_id", strconv.Itoa(n))
	case strings.TrimSpace(q.Text) != "":
		text := strings.ToLower(strings.TrimSpace(q.Text))
		if m := epTextRe.FindStringSubmatch(text); m != nil && strings.TrimSpace(m[1]) != "" {
			s, _ := strconv.Atoi(m[2])
			e, _ := strconv.Atoi(m[3])
			v.Set("query", strings.TrimSpace(m[1]))
			v.Set("season_number", strconv.Itoa(s))
			v.Set("episode_number", strconv.Itoa(e))
		} else {
			v.Set("query", text)
		}
	default:
		return nil, ErrNotFound
	}
	var r struct {
		Data []struct {
			Attributes struct {
				Language      string `json:"language"`
				DownloadCount int    `json:"download_count"`
				HashMatch     bool   `json:"moviehash_match"`
				HI            bool   `json:"hearing_impaired"`
				AI            bool   `json:"ai_translated"`
				Machine       bool   `json:"machine_translated"`
				Release       string `json:"release"`
				Files         []struct {
					FileID   int    `json:"file_id"`
					FileName string `json:"file_name"`
				} `json:"files"`
			} `json:"attributes"`
		} `json:"data"`
	}
	// the API wants sorted, lower-case parameters (otherwise it redirects)
	if err := c.do(ctx, http.MethodGet, "/subtitles?"+v.Encode(), nil, &r); err != nil {
		return nil, err
	}
	var out []Result
	for _, d := range r.Data {
		a := d.Attributes
		if len(a.Files) == 0 {
			continue
		}
		name := a.Files[0].FileName
		if name == "" {
			name = a.Release
		}
		out = append(out, Result{
			FileID: a.Files[0].FileID, Name: name, Lang: a.Language, Downloads: a.DownloadCount,
			HashMatch: a.HashMatch, HI: a.HI, Machine: a.AI || a.Machine,
		})
	}
	if len(out) == 0 {
		return nil, ErrNotFound
	}
	sort.SliceStable(out, func(i, j int) bool {
		a, b := out[i], out[j]
		if a.HashMatch != b.HashMatch {
			return a.HashMatch
		}
		if a.Machine != b.Machine {
			return !a.Machine
		}
		return a.Downloads > b.Downloads
	})
	return out[:min(len(out), MaxResults)], nil
}

// Download returns the file name and its UTF-8 text.
func (c *Client) Download(ctx context.Context, fileID int) (string, string, error) {
	var r struct {
		Link     string `json:"link"`
		FileName string `json:"file_name"`
	}
	if err := c.do(ctx, http.MethodPost, "/download", map[string]any{"file_id": fileID, "sub_format": "srt"}, &r); err != nil {
		return "", "", err
	}
	if r.Link == "" {
		return "", "", ErrNotFound
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, r.Link, nil)
	if err != nil {
		return "", "", err
	}
	req.Header.Set("User-Agent", userAgent)
	resp, err := c.HTTP.Do(req)
	if err != nil {
		return "", "", fmt.Errorf("altyazı indirilemedi: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return "", "", fmt.Errorf("altyazı indirilemedi: HTTP %d", resp.StatusCode)
	}
	b, err := io.ReadAll(io.LimitReader(resp.Body, MaxText+1))
	if err != nil {
		return "", "", err
	}
	if len(b) > MaxText {
		return "", "", errors.New("altyazı dosyası çok büyük")
	}
	return r.FileName, toUTF8(b), nil
}

// toUTF8: OpenSubtitles converts to UTF-8, but old Turkish files may still arrive
// as Windows-1254; decode those so ğ/ş/ı survive.
func toUTF8(b []byte) string {
	b = bytes.TrimPrefix(b, []byte{0xEF, 0xBB, 0xBF})
	if utf8.Valid(b) {
		return string(b)
	}
	var sb strings.Builder
	for _, c := range b {
		if r, ok := cp1254[c]; ok {
			sb.WriteRune(r)
		} else {
			sb.WriteRune(rune(c)) // Latin-1 range matches Windows-1254 elsewhere
		}
	}
	return sb.String()
}

// Windows-1254 bytes that differ from Latin-1.
var cp1254 = map[byte]rune{
	0x80: '€', 0x82: '‚', 0x83: 'ƒ', 0x84: '„', 0x85: '…', 0x86: '†', 0x87: '‡', 0x88: 'ˆ', 0x89: '‰',
	0x8A: 'Š', 0x8B: '‹', 0x8C: 'Œ', 0x91: '‘', 0x92: '’', 0x93: '“', 0x94: '”', 0x95: '•', 0x96: '–',
	0x97: '—', 0x98: '˜', 0x99: '™', 0x9A: 'š', 0x9B: '›', 0x9C: 'œ', 0x9F: 'Ÿ',
	0xD0: 'Ğ', 0xDD: 'İ', 0xDE: 'Ş', 0xF0: 'ğ', 0xFD: 'ı', 0xFE: 'ş',
}
