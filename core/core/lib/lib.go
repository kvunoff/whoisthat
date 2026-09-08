package lib

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"github.com/matoous/go-nanoid/v2"
	"os/exec"
	"strings"
	"unicode/utf8"
	"whoisthat-core/db"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

type profileMetaData struct {
	Name     string `json:"name"`
	Protocol string `json:"protocol"`
	Address  string `json:"address,omitzero"`
	Host     string `json:"host,omitzero"`
}

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

type batchProfileMetaData struct {
	Uri      string `json:"uri"`
	Name     string `json:"name"`
	Protocol string `json:"protocol"`
	Address  string `json:"address,omitzero"`
	Host     string `json:"host,omitzero"`
}

func GetDBAddProfileDatasFromStr(str string, group_id int) []structs.DBAddProfileData {
	str = decode64(str)

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

func getDBAddProfileDatasFromStrBatch(str string, group_id int) ([]structs.DBAddProfileData, error) {
	parserbin, err := utils.GetParserBin()
	if err != nil {
		return nil, fmt.Errorf("failed to find parser bin: %w", err)
	}

	cmd := exec.Command(parserbin, "--get-metadata-batch")
	cmd.Stdin = strings.NewReader(str)
	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("batch metadata failed: %w", err)
	}

	var batchItems []batchProfileMetaData
	if err := json.Unmarshal(out, &batchItems); err != nil {
		return nil, fmt.Errorf("unmarshaling batch metadata: %w", err)
	}

	profiles := make([]structs.DBAddProfileData, 0, len(batchItems))
	for _, item := range batchItems {
		profiles = append(profiles, structs.DBAddProfileData{
			Protocol: item.Protocol,
			Name:     item.Name,
			Address:  item.Address,
			Host:     item.Host,
			Uri:      item.Uri,
			GroupId:  group_id,
			NanoID:   generateNanoID(),
		})
	}
	return profiles, nil
}

func getDBAddProfileDataFromURI(uri string, group_id int) (structs.DBAddProfileData, error) {
	var profile_data structs.DBAddProfileData
	parserbin, err := utils.GetParserBin()
	if err != nil {
		return profile_data, fmt.Errorf("failed to find parser bin: %w", err)
	}
	parser_metadata_cmd := exec.Command(parserbin, uri, "--get-metadata")
	metadata_output, err := parser_metadata_cmd.Output()
	if err != nil {
		return profile_data, fmt.Errorf("getting metadata failed: %w (output: %s)", err, string(metadata_output))
	}

	var profile_metadata profileMetaData
	if err := json.Unmarshal(metadata_output, &profile_metadata); err != nil {
		return profile_data, fmt.Errorf("err unmarshaling metadata output: %w", err)
	}

	profile_data = structs.DBAddProfileData{
		Protocol: profile_metadata.Protocol,
		Name:     profile_metadata.Name,
		Address:  profile_metadata.Address,
		Host:     profile_metadata.Host,
		Uri:      uri,
		GroupId:  group_id,
		NanoID:   generateNanoID(),
	}
	return profile_data, nil
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
