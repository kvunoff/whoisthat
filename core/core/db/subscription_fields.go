package db

import (
	"strings"

	"whoisthat-core/structs"
)

type SubscriptionFields struct {
	ProviderInterval *int64
	LastUpdated      *int64
	Title            *string
	Upload           *int64
	Download         *int64
	Total            *int64
	Expires          *int64
	Format           *string
}

func (db *DB) MergeSubscriptionFields(group_id int, f SubscriptionFields) (structs.Group, error) {
	db.mu.Lock()
	defer db.mu.Unlock()

	group, err := db.loadGroupConfig(group_id)
	if err != nil {
		return group, err
	}
	if f.ProviderInterval != nil {
		group.SubUpdateInterval = *f.ProviderInterval
	}
	if f.LastUpdated != nil {
		group.SubLastUpdated = *f.LastUpdated
	}
	if f.Upload != nil {
		group.SubUpload = *f.Upload
	}
	if f.Download != nil {
		group.SubDownload = *f.Download
	}
	if f.Total != nil {
		group.SubTotal = *f.Total
	}
	if f.Expires != nil {
		group.SubExpires = *f.Expires
	}
	if f.Format != nil {
		group.SubscriptionFormat = *f.Format
	}
	if f.Title != nil {
		title := strings.TrimSpace(*f.Title)
		if title != "" {
			if group.Name == "" || group.Name == group.SubName {
				group.Name = title
			}
			group.SubName = title
		}
	}
	if err := db.updateGroup(group); err != nil {
		return group, err
	}
	return group, nil
}

// GetAllGroupConfigs returns group configs without loading their profiles. The
// auto-update scheduler runs often enough that decrypting every profile file
// just to read subscription state would be wasteful.
