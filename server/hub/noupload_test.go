package hub

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// The server accepts no file uploads: the old relay paths are gone.
func TestNoUploadEndpoint(t *testing.T) {
	srv := httptest.NewServer(New(Config{}).Handler())
	defer srv.Close()
	for _, p := range []string{"/relay", "/relay/x/y.mp3"} {
		res, err := http.Post(srv.URL+p, "application/octet-stream", strings.NewReader("data"))
		if err != nil {
			t.Fatal(err)
		}
		res.Body.Close()
		if res.StatusCode != http.StatusNotFound {
			t.Fatalf("POST %s = %d, want 404", p, res.StatusCode)
		}
	}
}
