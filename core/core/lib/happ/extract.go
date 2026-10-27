package happ

import (
	"fmt"
	gonanoid "github.com/matoous/go-nanoid/v2"
	"net/http"
	"net/url"
	"strings"
	"whoisthat-core/db"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
)

// ExtractLinksFromResponse extracts happ://routing/* links from the HTTP response headers.
func ExtractLinksFromResponse(resp *http.Response) []string {
	if resp == nil {
		return nil
	}
	var rawHeaders []string
	for k, v := range resp.Header {
		if strings.EqualFold(k, "routing") {
			rawHeaders = append(rawHeaders, v...)
		}
	}

	var links []string
	for _, h := range rawHeaders {
		val := h
		if strings.Contains(strings.ToLower(val), "%3a%2f%2f") {
			if dec, err := url.QueryUnescape(val); err == nil {
				val = dec
			}
		}
		links = append(links, ExtractLinksFromString(val)...)
	}

	return deduplicateLinks(links)
}

// ExtractLinks extracts happ://routing/* links from plain text or base64-decoded body content.
func ExtractLinks(body []byte) []string {
	var links []string
	s := string(body)

	// 1. Plain text search
	links = append(links, ExtractLinksFromString(s)...)

	// 2. Base64 decoded body search (if entire body or lines are base64)
	cleaned := strings.TrimSpace(s)
	if decoded, err := decodeBase64Tolerant(cleaned); err == nil {
		links = append(links, ExtractLinksFromString(string(decoded))...)
	}

	return deduplicateLinks(links)
}

// ExtractLinksFromString scans an arbitrary string for happ://routing/ links.
func ExtractLinksFromString(text string) []string {
	var links []string
	lower := strings.ToLower(text)
	target := "happ://routing/"

	startIdx := 0
	for {
		idx := strings.Index(lower[startIdx:], target)
		if idx == -1 {
			break
		}
		actualStart := startIdx + idx

		// Find end of link
		endIdx := len(text)
		for i := actualStart; i < len(text); i++ {
			c := text[i]
			if c == ' ' || c == '\t' || c == '\r' || c == '\n' ||
				c == '"' || c == '\'' || c == ';' || c == ',' ||
				c == '<' || c == '>' || c == ')' {
				endIdx = i
				break
			}
		}

		rawLink := text[actualStart:endIdx]
		// Clean trailing punctuation
		rawLink = strings.TrimRight(rawLink, ".,;\"'>)")

		if strings.HasPrefix(strings.ToLower(rawLink), target) {
			links = append(links, rawLink)
		}

		startIdx = actualStart + len(target)
	}

	return links
}

func deduplicateLinks(links []string) []string {
	seen := make(map[string]bool)
	var res []string
	for _, l := range links {
		trimmed := strings.TrimSpace(l)
		if trimmed == "" {
			continue
		}
		if !seen[trimmed] {
			seen[trimmed] = true
			res = append(res, trimmed)
		}
	}
	return res
}

// UpsertFromLink parses a happ://routing link and upserts the profile into database for the specified group.
// Returns (profile, isOff, error).
func UpsertFromLink(database *db.DB, groupID int, link string, now int64) (structs.RoutingProfile, bool, error) {
	parsed, err := ParseLink(link)
	if err != nil {
		return structs.RoutingProfile{}, false, fmt.Errorf("failed to parse link: %w", err)
	}

	if parsed.Action == ActionOff {
		_ = database.SetActiveRoutingProfile(groupID, "")
		return structs.RoutingProfile{}, true, nil
	}

	if parsed.Profile == nil {
		return structs.RoutingProfile{}, false, fmt.Errorf("missing profile in link: %s", link)
	}

	cfg, warnings, err := ToRoutingConfig(*parsed.Profile)
	if err != nil {
		return structs.RoutingProfile{}, false, fmt.Errorf("invalid routing config: %w", err)
	}
	for _, w := range warnings {
		logger.Warnf("happ routing %q: %s", parsed.Profile.Name, w)
	}

	globalProxy := true
	if parsed.Profile.GlobalProxy != nil {
		globalProxy = parsed.Profile.GlobalProxy.Value()
	}

	existing, err := database.FindRoutingProfileByName(groupID, parsed.Profile.Name)
	if err == nil {
		// Existing profile found
		if parsed.Profile.LastUpdated.Value() > 0 && existing.LastUpdated > 0 && parsed.Profile.LastUpdated.Value() < existing.LastUpdated {
			return existing, false, nil // skip older update
		}

		existing.RawJSON = parsed.RawJSON
		existing.Rules = cfg.Rules
		existing.DomainStrategy = cfg.DomainStrategy
		existing.GlobalProxy = globalProxy
		existing.DNS = cfg.DNS
		existing.LastUpdated = parsed.Profile.LastUpdated.Value()
		existing.UpdatedAt = now

		if parsed.Profile.Geoipurl == "" && parsed.Profile.Geositeurl == "" {
			existing.State = "ready"
			existing.GeoDir = ""
		} else {
			existing.State = "updating"
		}

		if err := database.SaveRoutingProfile(existing); err != nil {
			return existing, false, fmt.Errorf("failed to save routing profile: %w", err)
		}

		if parsed.Action == ActionOnAdd && existing.State == "ready" {
			_ = database.SetActiveRoutingProfile(groupID, existing.Id)
		}

		return existing, false, nil
	}

	// New profile
	newID := gonanoid.Must()
	newProf := structs.RoutingProfile{
		Id:             newID,
		GroupId:        groupID,
		Name:           parsed.Profile.Name,
		Source:         "happ",
		RawJSON:        parsed.RawJSON,
		Rules:          cfg.Rules,
		DomainStrategy: cfg.DomainStrategy,
		GlobalProxy:    globalProxy,
		DNS:            cfg.DNS,
		LastUpdated:    parsed.Profile.LastUpdated.Value(),
		CreatedAt:      now,
		UpdatedAt:      now,
		State:          "ready",
	}

	if parsed.Profile.Geoipurl != "" || parsed.Profile.Geositeurl != "" {
		newProf.State = "updating"
	}

	if err := database.SaveRoutingProfile(newProf); err != nil {
		return newProf, false, fmt.Errorf("failed to save new routing profile: %w", err)
	}

	group, gErr := database.LoadGroupConfig(groupID)
	if gErr == nil && newProf.State == "ready" {
		if parsed.Action == ActionOnAdd || group.ActiveRoutingProfileId == "" {
			_ = database.SetActiveRoutingProfile(groupID, newProf.Id)
		}
	}

	return newProf, false, nil
}
