// koni-server is the KONİ Santral: WebSocket room sync, and with -db accounts and the
// events log (SQLite).
//
//	koni-server [-addr :8080] [-db koni.db] [-reports dir] [-avatars dir]
//	koni-server invite -db koni.db [-ttl 168h]   # prints a one-time account invite code
//	koni-server admin -db koni.db [-off] Ad#1234  # makes (or unmakes) an admin
//	koni-server admin -db koni.db -list           # lists accounts
//
// Optional services read their keys from the environment (never flags: those show up
// in ps): KONI_TMDB_KEY (+ KONI_OMDB_KEY) turns on "Ne izlesek?", KONI_OS_KEY (+
// KONI_OS_USER, KONI_OS_PASS) turns on OpenSubtitles search.
package main

import (
	"flag"
	"fmt"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"time"

	"github.com/serkanqltun-hub/koni/server/discover"
	"github.com/serkanqltun-hub/koni/server/hub"
	"github.com/serkanqltun-hub/koni/server/store"
	"github.com/serkanqltun-hub/koni/server/subs"
)

func main() {
	if len(os.Args) > 1 && os.Args[1] == "invite" {
		invite(os.Args[2:])
		return
	}
	if len(os.Args) > 1 && os.Args[1] == "admin" {
		admin(os.Args[2:])
		return
	}
	addr := flag.String("addr", ":8080", "listen address")
	db := flag.String("db", "", "SQLite database: enables accounts and the events log (empty: open server)")
	reports := flag.String("reports", "", "bug report directory (empty: reports/ next to -db; neither: bug reports off)")
	avatars := flag.String("avatars", "", "profile picture directory (empty: avatars/ next to -db)")
	flag.Parse()
	cfg := hub.Config{Reports: *reports, Avatars: *avatars}
	mode := "open (no accounts)"
	if *db != "" {
		st, err := store.Open(*db)
		if err != nil {
			log.Fatalf("db: %v", err)
		}
		defer st.Close()
		cfg.Store = st
		mode = "accounts, db " + *db
		if cfg.Reports == "" {
			cfg.Reports = filepath.Join(filepath.Dir(*db), "reports")
		}
		if cfg.Avatars == "" {
			cfg.Avatars = filepath.Join(filepath.Dir(*db), "avatars")
		}
		mode += ", avatars " + cfg.Avatars
	}
	if cfg.Reports != "" {
		mode += ", reports " + cfg.Reports
	}
	if k := os.Getenv("KONI_TMDB_KEY"); k != "" {
		d, err := discover.New(k, os.Getenv("KONI_OMDB_KEY"))
		if err != nil {
			log.Fatalf("discover: %v", err)
		}
		cfg.Discover = d
		mode += ", discover"
		if os.Getenv("KONI_OMDB_KEY") != "" {
			mode += "+omdb"
		}
	}
	if k := os.Getenv("KONI_OS_KEY"); k != "" {
		cfg.Subs = subs.New(k, os.Getenv("KONI_OS_USER"), os.Getenv("KONI_OS_PASS"))
		mode += ", subtitles"
	}
	h := hub.New(cfg)
	srv := &http.Server{Addr: *addr, Handler: h.Handler(), ReadHeaderTimeout: 10 * time.Second}
	log.Printf("koni-server %s listening on %s, %s", hub.ProtocolVersion, *addr, mode)
	log.Fatal(srv.ListenAndServe())
}

func invite(args []string) {
	fs := flag.NewFlagSet("invite", flag.ExitOnError)
	db := fs.String("db", "", "SQLite database (required)")
	ttl := fs.Duration("ttl", 7*24*time.Hour, "validity")
	fs.Parse(args)
	if *db == "" {
		fs.Usage()
		os.Exit(2)
	}
	st, err := store.Open(*db)
	if err != nil {
		log.Fatalf("db: %v", err)
	}
	defer st.Close()
	code, err := st.CreateInvite(0, *ttl)
	if err != nil {
		log.Fatalf("invite: %v", err)
	}
	fmt.Println(code)
}

func admin(args []string) {
	fs := flag.NewFlagSet("admin", flag.ExitOnError)
	db := fs.String("db", "", "SQLite database (required)")
	off := fs.Bool("off", false, "take admin away")
	list := fs.Bool("list", false, "list accounts")
	fs.Parse(args)
	if *db == "" || (!*list && fs.NArg() != 1) {
		fs.Usage()
		os.Exit(2)
	}
	st, err := store.Open(*db)
	if err != nil {
		log.Fatalf("db: %v", err)
	}
	defer st.Close()
	if *list {
		ms, err := st.Members()
		if err != nil {
			log.Fatalf("list: %v", err)
		}
		for _, m := range ms {
			flag := ""
			if m.Admin {
				flag = " (admin)"
			}
			fmt.Println(m.Name + flag)
		}
		return
	}
	u, err := st.SetAdmin(fs.Arg(0), !*off)
	if err != nil {
		log.Fatalf("admin: %v", err)
	}
	fmt.Printf("%s admin: %v\n", u.Handle(), !*off)
}
