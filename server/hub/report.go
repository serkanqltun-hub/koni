package hub

import (
	"errors"
	"fmt"
	"io/fs"
	"log"
	"os"
	"path/filepath"
	"strings"
	"time"
)

const (
	maxReportFrame = 256 << 10 // read limit for the bug_report handshake only
	maxReportText  = 200 << 10
	reportsPerHour = 5
	maxVersion     = 32
)

var trt = time.FixedZone("TRT", 3*3600)

// BugReport answers a `bug_report` handshake (PROTOCOL.md §3.0b): the client's log
// is written as a plain text file under Config.Reports.
func (h *Hub) BugReport(in Inbound, ip string) map[string]any {
	if h.cfg.Reports == "" {
		return errMsg("no_reports")
	}
	if len(in.Text) > maxReportText {
		return errMsg("too_large")
	}
	name := "anonim"
	if st := h.cfg.Store; st != nil {
		if in.Auth == "" {
			return errMsg("auth_required")
		}
		u, err := st.UserByToken(in.Auth)
		if err != nil {
			return errMsg(errCode(err))
		}
		name = u.Name
	} else if n, ok := validName(in.Username); ok {
		name = n
	}
	now := h.cfg.Now()
	if !h.reports.allow(ip, now) {
		return errMsg("rate_limited")
	}
	version := strings.Join(strings.Fields(in.Version), " ")
	if r := []rune(version); len(r) > maxVersion {
		version = string(r[:maxVersion])
	}
	head := fmt.Sprintf("Kullanıcı: %s\nSürüm: %s\nIP: %s\nZaman: %s\n\n", name, version, ip, now.In(trt).Format(time.RFC3339))
	id, err := writeReport(h.cfg.Reports, now.In(trt).Format("20060102-150405")+"-"+fileSafe(name), head+in.Text)
	if err != nil {
		log.Printf("bug_report: %v", err)
		return errMsg("server_error")
	}
	return map[string]any{"t": "bug_report_ok", "id": id}
}

// writeReport creates base.txt (base-2.txt, … if taken) in dir and returns the file name.
func writeReport(dir, base, body string) (string, error) {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return "", err
	}
	for i := 1; i <= 20; i++ {
		id := base + ".txt"
		if i > 1 {
			id = fmt.Sprintf("%s-%d.txt", base, i)
		}
		f, err := os.OpenFile(filepath.Join(dir, id), os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
		if errors.Is(err, fs.ErrExist) {
			continue
		}
		if err != nil {
			return "", err
		}
		_, err = f.WriteString(body)
		if cerr := f.Close(); err == nil {
			err = cerr
		}
		return id, err
	}
	return "", errors.New("no free file name for " + base)
}

// fileSafe lowercases name and keeps only ASCII letters, digits, '-' and '_'
// (Turkish letters fold to their ASCII base).
func fileSafe(name string) string {
	var b strings.Builder
	for _, c := range strings.ToLower(name) {
		switch c {
		case 'ç':
			c = 'c'
		case 'ğ':
			c = 'g'
		case 'ı':
			c = 'i'
		case 'ö':
			c = 'o'
		case 'ş':
			c = 's'
		case 'ü':
			c = 'u'
		}
		if c >= 'a' && c <= 'z' || c >= '0' && c <= '9' || c == '-' || c == '_' {
			b.WriteRune(c)
		}
	}
	if b.Len() == 0 {
		return "anonim"
	}
	return b.String()
}
