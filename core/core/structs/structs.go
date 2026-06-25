package structs

import "encoding/json"

// database types
type DBAddProfileData struct {
	Uri      string `json:"uri"`
	GroupId  int    `json:"group_id"`
	Name     string `json:"name"`
	Protocol string `json:"protocol"`
	Address  string `json:"address,omitzero"`
	Host     string `json:"host,omitzero"`
	NanoID   string `json:"nano-id,omitzero"`
}

// commands and responses
type DeleteProfilesData struct {
	Profiles []ProfileID `json:"profiles"`
}

type ProfilesDeleted struct {
	DeletedProfiles []ProfileID `json:"deleted-profiles"`
}

type AddProfilesData struct {
	Uris    string `json:"uris"`
	GroupId int    `json:"group_id"`
}

type ProfilesAdded struct {
	Profiles []Profile `json:"profiles"`
}

type AddGroupData struct {
	Name            string `json:"name"`
	SubscriptionUrl string `json:"subscription_url"`
}

type GroupAdded struct {
	Id              int    `json:"id"`
	SubscriptionUrl string `json:"subscription_url"`
	Name            string `json:"name"`
}

type DeleteGroupData struct {
	Id int `json:"id"`
}

type GroupDeleted struct {
	Id int `json:"id"`
}

type ConnectData struct {
	Profile ProfileID `json:"profile"`
}

type DisconnectData struct{}

type TestProfileData struct {
	Profile ProfileID `json:"profile"`
	Method  string    `json:"method"`
}

type ProfileUpdated struct {
	Profile Profile `json:"profile"`
}

type GetApplicationStateData struct{}

type ApplicationState struct {
	Groups           []GroupWithProfiles `json:"groups"`
	ConnectionStatus ProxyStatus         `json:"connection-status"`
	TunStatus        bool                `json:"tun-status"`
	HwidInfo         HwidData            `json:"hwid_info"`
	KillSwitch       bool                `json:"kill_switch"`
}

type UpdateSubscriptionData struct {
	GroupId int `json:"group_id"`
}

type SubscriptionUpdated struct {
	GroupId  int       `json:"group_id"`
	Group    Group     `json:"group"`
	Profiles []Profile `json:"profiles"`
}

type Warning struct {
	Key     string `json:"key"`
	Content string `json:"content"`
}

type DieData struct{}

type IsRootData struct{}
type IsRootAnswer struct {
	IsRoot bool
}

type HwidData struct {
	Enabled   bool   `json:"enabled"`
	Hwid      string `json:"hwid"`
	UserAgent string `json:"user_agent"`
	Platform  string `json:"platform"`
	Kernel    string `json:"kernel"`
	Model     string `json:"model"`
}

type SetHwidData struct {
	Enabled   *bool   `json:"enabled,omitempty"`
	UserAgent *string `json:"user_agent,omitempty"`
	Reset     bool    `json:"reset"`
}

type SetTunNameData struct {
	TunName string `json:"tun_name"`
}

type UpdateProfileData struct {
	Profile ProfileID
	Name    string
}

type UpdateGroupData struct {
	Id              int    `json:"id"`
	Name            string `json:"name,omitzero"`
	SubscriptionUrl string `json:"subscription_url,omitzero"`
}

type SetKillSwitchData struct {
	Enabled bool `json:"enabled"`
}

// general types
type DBConfig struct {
	LastGroupId int `json:"last_group_id"`
}

type GroupWithProfiles struct {
	Group    Group     `json:"group"`
	Profiles []Profile `json:"profiles"`
}

type Group struct {
	Id              int    `json:"id"`
	SubscriptionUrl string `json:"subscription_url"`
	Name            string `json:"name"`
	LastId          int    `json:"last_id"`
	SubLastUpdated  int64  `json:"sub_last_updated,omitzero"`
	SubExpires      int64  `json:"sub_expires,omitzero"`
	SubUpload       int64  `json:"sub_upload,omitzero"`
	SubDownload     int64  `json:"sub_download,omitzero"`
	SubTotal        int64  `json:"sub_total,omitzero"`
}

type Profile struct {
	Id         int    `json:"id"`
	GroupId    int    `json:"group_id"`
	NanoID     string `json:"nano-id,omitzero"`
	Name       string `json:"name"`
	Protocol   string `json:"protocol"`
	Uri        string `json:"uri"`
	Address    string `json:"address,omitzero"`
	Host       string `json:"host,omitzero"`
	TestResult int    `json:"test-result"`
}

type ProfileID struct {
	Id      int `json:"id"`
	GroupId int `json:"group_id"`
}

type TCPMessage struct {
	Msg  string          `json:"msg"`
	Data json.RawMessage `json:"data"`
}

type Message[T any] struct {
	Msg  string `json:"msg"`
	Data T      `json:"data"`
}

type ProxyStatus struct {
	Connection  string  `json:"connection"`
	Profile     Profile `json:"profile"`
	ConnectedAt int64   `json:"connected_at"`
}

type DisableTunData struct{}
type EnableTunData struct{}

type TunStatus struct {
	IsEnabled bool `json:"is_enabled"`
}

type TrafficStats struct {
	ProxyUp   int64 `json:"proxy_up"`
	ProxyDown int64 `json:"proxy_down"`
	DirectUp  int64 `json:"direct_up"`
	DirectDown int64 `json:"direct_down"`
}

// routing types
type RoutingRule struct {
	Type        string `json:"type"`
	Domain      string `json:"domain,omitzero"`
	IP          string `json:"ip,omitzero"`
	Protocol    string `json:"protocol,omitzero"`
	Port        string `json:"port,omitzero"`
	OutboundTag string `json:"outbound_tag"`
	Enabled     bool   `json:"enabled"`
}

type RoutingConfig struct {
	DomainStrategy string        `json:"domain_strategy"`
	Rules          []RoutingRule `json:"rules"`
}

type GetRoutingData struct{}
type UpdateRoutingData struct {
	Config RoutingConfig `json:"config"`
}

type RoutingUpdated struct {
	Config RoutingConfig `json:"config"`
}
