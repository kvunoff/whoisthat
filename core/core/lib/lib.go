package lib

import (
	"encoding/base64"
	"strings"
	"unicode"
	"unicode/utf8"

	"github.com/matoous/go-nanoid/v2"
	"whoisthat-core/db"
	"whoisthat-core/lib/parser"
	"whoisthat-core/lib/xrayjson"
	"whoisthat-core/structs"
)

func AddProfiles(DB *db.DB, data structs.AddProfilesData) structs.ProfilesAdded {
	profiles_data := GetDBAddProfileDatasFromStr(data.Uris, data.GroupId)
	var profiles []structs.Profile
	for _, profile_data := range profiles_data {
		profile_added, err := DB.AddProfile(profile_data)
		if err == nil {
			profiles = append(profiles, profile_added)
		}
	}
	return structs.ProfilesAdded{
		Profiles: profiles,
	}
}

func GetDBAddProfileDatasFromStr(str string, group_id int) []structs.DBAddProfileData {
	str = decode64(str)

	// JSON subscriptions carry one full Xray config per server instead of
	// share links. They must be detected before the URI parsers see the body.
	if configs, err := xrayjson.Parse([]byte(str)); err == nil && len(configs) > 0 {
		return configProfilesToDBData(configs, group_id)
	}

	if profiles, err := getDBAddProfileDatasFromStrBatch(str, group_id); err == nil && len(profiles) > 0 {
		return profiles
	}

	uris := strings.FieldsSeq(str)
	var profiles []structs.DBAddProfileData
	for uri := range uris {
		profile, err := getDBAddProfileDataFromURI(uri, group_id)
		if err != nil {
			continue
		}
		profiles = append(profiles, profile)
	}
	return profiles
}

func configProfilesToDBData(configs []xrayjson.Profile, group_id int) []structs.DBAddProfileData {
	profiles := make([]structs.DBAddProfileData, 0, len(configs))
	for _, config := range configs {
		profiles = append(profiles, structs.DBAddProfileData{
			Name:        config.Name,
			Protocol:    config.Protocol,
			Address:     config.Address,
			Host:        config.Host,
			Port:        config.Port,
			GroupId:     group_id,
			NanoID:      generateNanoID(),
			Config:      config.Raw,
			ProviderKey: config.ProviderKey,
			Managed:     true,
		})
	}
	return profiles
}

func getDBAddProfileDatasFromStrBatch(str string, group_id int) ([]structs.DBAddProfileData, error) {
	batchItems, err := parser.GetMetadataBatch(strings.NewReader(str))
	if err != nil {
		return nil, err
	}

	profiles := make([]structs.DBAddProfileData, 0, len(batchItems))
	for _, item := range batchItems {
		addr := ""
		if item.Address != nil {
			addr = *item.Address
		}
		host := ""
		if item.Host != nil {
			host = *item.Host
		}
		profiles = append(profiles, structs.DBAddProfileData{
			Protocol: item.Protocol,
			Name:     item.Name,
			Address:  addr,
			Host:     host,
			Uri:      item.URI,
			GroupId:  group_id,
			NanoID:   generateNanoID(),
		})
	}
	return profiles, nil
}

func getDBAddProfileDataFromURI(uri string, group_id int) (structs.DBAddProfileData, error) {
	meta, err := parser.GetMetadata(uri)
	if err != nil {
		return structs.DBAddProfileData{}, err
	}

	addr := ""
	if meta.Address != nil {
		addr = *meta.Address
	}
	host := ""
	if meta.Host != nil {
		host = *meta.Host
	}

	profile_data := structs.DBAddProfileData{
		Protocol: meta.Protocol,
		Name:     meta.Name,
		Address:  addr,
		Host:     host,
		Uri:      uri,
		GroupId:  group_id,
		NanoID:   generateNanoID(),
	}
	return profile_data, nil
}

// SubscriptionHeader returns the value of a "#key: value" or "key: value"
// line in a subscription body. Some providers put profile metadata in the body
// instead of (or in addition to) HTTP headers; the body may be base64 encoded.
func SubscriptionHeader(body, key string) (string, bool) {
	if v, ok := scanSubscriptionHeader(body, key); ok {
		return v, true
	}
	clean := strings.Map(func(r rune) rune {
		if unicode.IsSpace(r) {
			return -1
		}
		return r
	}, strings.TrimSpace(body))
	if clean == "" {
		return "", false
	}
	return scanSubscriptionHeader(decode64(clean), key)
}

func scanSubscriptionHeader(text, key string) (string, bool) {
	for _, line := range strings.Split(text, "\n") {
		line = strings.TrimPrefix(strings.TrimSpace(line), "#")
		k, v, ok := strings.Cut(line, ":")
		if !ok || !strings.EqualFold(strings.TrimSpace(k), key) {
			continue
		}
		return strings.Trim(strings.TrimSpace(v), `"'`), true
	}
	return "", false
}

func decode64(str string) string {
	clean := strings.TrimSpace(str)
	encodings := []*base64.Encoding{
		base64.StdEncoding,
		base64.RawStdEncoding,
		base64.URLEncoding,
		base64.RawURLEncoding,
	}
	for _, enc := range encodings {
		if decoded, err := enc.DecodeString(clean); err == nil && utf8.Valid(decoded) {
			return string(decoded)
		}
	}
	return str
}

func generateNanoID() string {
	return gonanoid.Must()
}
