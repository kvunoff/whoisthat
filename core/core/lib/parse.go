package lib

import (
	"whoisthat-core/utils"
	"fmt"
	"os/exec"
	"strconv"
)

func ParseUri(uri string, socksport int, httpport int) ([]byte, error) {
	var parsed_config []byte
	parserbin, err := utils.GetParserBin()
	if err != nil {
		return parsed_config, fmt.Errorf("failed to parse: %w", err)
	}
	var parser_parse_cmd *exec.Cmd
	if httpport != -1 {
		parser_parse_cmd = exec.Command(parserbin, uri, "--socksport", strconv.Itoa(socksport), "--httpport", strconv.Itoa(httpport))
	} else {
		parser_parse_cmd = exec.Command(parserbin, uri, "--socksport", strconv.Itoa(socksport))
	}

	parsed_config, err = parser_parse_cmd.Output()
	if err != nil {
		return parsed_config, fmt.Errorf("parsing uri fialed: %w", err)
	}

	return parsed_config, nil
}
