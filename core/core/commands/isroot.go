package cmd

import (
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

func (cmd *Cmd) IsRoot(data structs.IsRootData) {
	is_root := utils.IsRoot()
	cmd.send("is-root-answer", structs.IsRootAnswer{IsRoot: is_root})
}
