package db

import (
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
	"whoisthat-core/lib/logger"
)

const (
	snapshotPrefix = "db-"
	snapshotKeep   = 3
)

func (db *DB) backupsDir() string {
	return filepath.Join(filepath.Dir(db.Path), "backups")
}

func (db *DB) snapshotDays() []string {
	entries, err := os.ReadDir(db.backupsDir())
	if err != nil {
		return nil
	}
	var days []string
	for _, e := range entries {
		if e.IsDir() && strings.HasPrefix(e.Name(), snapshotPrefix) {
			days = append(days, strings.TrimPrefix(e.Name(), snapshotPrefix))
		}
	}
	sort.Strings(days)
	return days
}

func (db *DB) latestSnapshotDay() string {
	days := db.snapshotDays()
	if len(days) == 0 {
		return ""
	}
	return days[len(days)-1]
}

// snapshotDaily keeps one copy of the database per day (the last three), so a
// damaged or undecryptable database can be restored from the UI with one click.
// Skipped for test databases (no dedicated key path) and when the current key
// already fails to open the files: that copy would be worthless.
func (db *DB) snapshotDaily() {
	day := time.Now().Format("20060102")
	for _, d := range db.snapshotDays() {
		if d == day {
			return
		}
	}

	dest := filepath.Join(db.backupsDir(), snapshotPrefix+day)
	tmp := dest + ".tmp"
	_ = os.RemoveAll(tmp)
	if err := copyTree(db.Path, tmp); err != nil {
		logger.Warnf("db: daily snapshot failed: %v", err)
		_ = os.RemoveAll(tmp)
		return
	}
	_ = os.RemoveAll(dest)
	if err := os.Rename(tmp, dest); err != nil {
		logger.Warnf("db: daily snapshot rename failed: %v", err)
		_ = os.RemoveAll(tmp)
		return
	}
	logger.Infof("db: snapshot saved to %s", dest)

	days := db.snapshotDays()
	for len(days) > snapshotKeep {
		_ = os.RemoveAll(filepath.Join(db.backupsDir(), snapshotPrefix+days[0]))
		days = days[1:]
	}
}

// RestoreSnapshot replaces the database with the newest daily snapshot and
// keeps the unreadable copy next to it for inspection.
func (db *DB) RestoreSnapshot() (string, error) {
	db.mu.Lock()
	defer db.mu.Unlock()

	days := db.snapshotDays()
	if len(days) == 0 {
		return "", errors.New("no database backup available yet")
	}
	day := days[len(days)-1]
	src := filepath.Join(db.backupsDir(), snapshotPrefix+day)

	broken := db.Path + ".broken-" + time.Now().Format("20060102-150405")
	if err := os.Rename(db.Path, broken); err != nil {
		if !os.IsNotExist(err) {
			return "", fmt.Errorf("cannot move the current database aside: %w", err)
		}
		broken = ""
	}
	if err := copyTree(src, db.Path); err != nil {
		if broken != "" {
			_ = os.Rename(broken, db.Path)
		}
		return "", fmt.Errorf("cannot restore the database backup: %w", err)
	}

	db.degraded.Store(false)
	db.degradedErr.Store("")
	logger.Warnf("db: restored snapshot %s (previous database kept at %s)", day, broken)
	return day, nil
}

// copyTree copies a directory recursively, keeping only files and dirs and
// forcing owner-only permissions.
func copyTree(src, dst string) error {
	return filepath.Walk(src, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		target := filepath.Join(dst, rel)
		if info.IsDir() {
			return os.MkdirAll(target, 0700)
		}
		if !info.Mode().IsRegular() {
			return nil
		}
		return copyFile(path, target)
	})
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()

	if err := os.MkdirAll(filepath.Dir(dst), 0700); err != nil {
		return err
	}
	out, err := os.OpenFile(dst, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, 0600)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		return err
	}
	return out.Close()
}
