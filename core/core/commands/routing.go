package cmd

import (
	"whoisthat-core/structs"
	"log"
)

func (cmd *Cmd) GetRouting(data structs.GetRoutingData) {
	cfg, err := cmd.DB.LoadRouting()
	if err != nil {
		log.Printf("failed to load routing: %v", err)
		cmd.warn("get-routing-failed", "failed to load routing config")
		return
	}
	cmd.send("routing-updated", structs.RoutingUpdated{Config: cfg})
}

func (cmd *Cmd) UpdateRouting(data structs.UpdateRoutingData) {
	if err := cmd.DB.SaveRouting(data.Config); err != nil {
		log.Printf("failed to save routing: %v", err)
		cmd.warn("update-routing-failed", "failed to save routing config")
		return
	}
	cmd.send("routing-updated", structs.RoutingUpdated{Config: data.Config})
}
