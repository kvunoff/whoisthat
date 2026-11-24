package structs

type AntiDPIFragment struct {
	Enabled  bool   `json:"enabled"`
	Packets  string `json:"packets"`
	Length   string `json:"length"`
	Interval string `json:"interval"`
}

type AntiDPINoise struct {
	Enabled bool   `json:"enabled"`
	Type    string `json:"type"`
	Packet  string `json:"packet"`
	Delay   string `json:"delay"`
}

type AntiDPIConfig struct {
	Fragment AntiDPIFragment `json:"fragment"`
	Noise    AntiDPINoise    `json:"noise"`
}

type SetAntiDPIData struct {
	Config AntiDPIConfig `json:"config"`
}

type AntiDPIUpdated struct {
	Config AntiDPIConfig `json:"config"`
}
