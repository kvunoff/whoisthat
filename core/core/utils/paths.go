package utils

import (
	"fmt"
	"os"
	osuser "os/user"
	"strconv"
)

func GetParserBin() (string, error) {
	return GetBinPath("whoisthat-parser")
}

func GetTun2socksBin() (string, error) {
	return GetBinPath("tun2socks")
}

func GetXrayBin() (string, error) {
	return GetBinPath("xray")
}

func GetHomeDir() (string, error) {
	uid := os.Getuid()
	if uid == 0 {
		uidStr := os.Getenv("SUDO_UID")
		if uidStr == "" {
			uidStr = os.Getenv("PKEXEC_UID")
		}
		real_uid, err := strconv.Atoi(uidStr)
		if err != nil {
			return "", fmt.Errorf("failed to get user id outside of sudo/pkexec %w", err)
		}
		uid = real_uid
	}

	user, err := osuser.LookupId(strconv.Itoa(uid))
	if err != nil {
		// Don't Fatal: a missing/odd UID mapping shouldn't kill a running
		// daemon mid-session. Surface the error to the caller.
		return "", fmt.Errorf("failed to get user from uid %d: %w", uid, err)
	}
	return user.HomeDir, nil
}

func RemoveDuplicates(input []string) []string {
	seen := make(map[string]struct{})
	result := []string{}

	for _, v := range input {
		if _, ok := seen[v]; !ok {
			seen[v] = struct{}{}
			result = append(result, v)
		}
	}
	return result
}
