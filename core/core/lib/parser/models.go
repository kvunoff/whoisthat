package parser

// Config represents the top-level Xray configuration.
type Config struct {
	Outbounds []Outbound `json:"outbounds"`
	Inbounds  []Inbound  `json:"inbounds"`
}

// Outbound defines an outbound proxy connection in Xray.
type Outbound struct {
	Protocol       string         `json:"protocol"`
	Tag            string         `json:"tag"`
	Settings       any            `json:"settings"`
	StreamSettings StreamSettings `json:"streamSettings"`
}

// Inbound defines an inbound proxy listener in Xray.
type Inbound struct {
	Listen   string            `json:"listen"`
	Port     uint16            `json:"port"`
	Protocol string            `json:"protocol"`
	Tag      string            `json:"tag"`
	Settings *InboundSettings  `json:"settings,omitempty"`
	Sniffing *SniffingSettings `json:"sniffing,omitempty"`
}

// InboundSettings specifies inbound protocol options.
type InboundSettings struct {
	UDP bool `json:"udp"`
}

// SniffingSettings configures protocol sniffing on inbounds.
type SniffingSettings struct {
	Enabled         *bool    `json:"enabled,omitempty"`
	DestOverride    []string `json:"destOverride,omitempty"`
	DomainsExcluded []string `json:"domainsExcluded,omitempty"`
	MetadataOnly    *bool    `json:"metadataOnly,omitempty"`
	RouteOnly       *bool    `json:"routeOnly,omitempty"`
}

// VnextUser represents a user in a vnext server object (VLESS/VMess).
type VnextUser struct {
	ID         *string `json:"id,omitempty"`
	Encryption *string `json:"encryption,omitempty"`
	Flow       *string `json:"flow,omitempty"`
	Level      *uint8  `json:"level,omitempty"`
	Security   *string `json:"security,omitempty"`
}

// VnextServerObject represents a server in VLESS/VMess outbound settings.
type VnextServerObject struct {
	Address *string     `json:"address,omitempty"`
	Port    *uint16     `json:"port,omitempty"`
	Users   []VnextUser `json:"users,omitempty"`
}

// VlessOutboundSettings holds settings for VLESS outbound.
type VlessOutboundSettings struct {
	Vnext []VnextServerObject `json:"vnext"`
}

// VmessOutboundSettings holds settings for VMess outbound.
type VmessOutboundSettings struct {
	Vnext []VnextServerObject `json:"vnext"`
}

// TrojanServerObject represents a server in Trojan outbound settings.
type TrojanServerObject struct {
	Address  *string `json:"address,omitempty"`
	Port     *uint16 `json:"port,omitempty"`
	Password *string `json:"password,omitempty"`
	Level    *uint8  `json:"level,omitempty"`
}

// TrojanOutboundSettings holds settings for Trojan outbound.
type TrojanOutboundSettings struct {
	Servers []TrojanServerObject `json:"servers"`
}

// ShadowSocksServerObject represents a server in Shadowsocks outbound settings.
type ShadowSocksServerObject struct {
	Address  *string `json:"address,omitempty"`
	Port     *uint16 `json:"port,omitempty"`
	Password *string `json:"password,omitempty"`
	Level    *uint8  `json:"level,omitempty"`
	Method   *string `json:"method,omitempty"`
}

// ShadowSocksOutboundSettings holds settings for Shadowsocks outbound.
type ShadowSocksOutboundSettings struct {
	Servers []ShadowSocksServerObject `json:"servers"`
}

// SocksUser represents user authentication for SOCKS.
type SocksUser struct {
	User *string `json:"user,omitempty"`
	Pass *string `json:"pass,omitempty"`
}

// SocksServerObject represents a server in SOCKS outbound settings.
type SocksServerObject struct {
	Address *string     `json:"address,omitempty"`
	Port    *uint16     `json:"port,omitempty"`
	Level   *uint8      `json:"level,omitempty"`
	Users   []SocksUser `json:"users,omitempty"`
}

// SocksOutboundSettings holds settings for SOCKS outbound.
type SocksOutboundSettings struct {
	Servers []SocksServerObject `json:"servers"`
}

// HysteriaOutboundSettings holds native Xray hysteria outbound settings.
type HysteriaOutboundSettings struct {
	Version uint32  `json:"version"`
	Address *string `json:"address,omitempty"`
	Port    *uint16 `json:"port,omitempty"`
}

// NonHeaderObject represents header configuration for QUIC.
type NonHeaderObject struct {
	Type *string `json:"type,omitempty"`
}

// QuicSettings holds transport settings for QUIC.
type QuicSettings struct {
	Header   *NonHeaderObject `json:"header,omitempty"`
	Security *string          `json:"security,omitempty"`
	Key      *string          `json:"key,omitempty"`
}

// GRPCSettings holds transport settings for gRPC.
type GRPCSettings struct {
	Authority   *string `json:"authority,omitempty"`
	MultiMode   *bool   `json:"multiMode,omitempty"`
	ServiceName *string `json:"serviceName,omitempty"`
}

// KCPSettings holds transport settings for mKCP.
type KCPSettings struct {
	MTU             *uint32 `json:"mtu,omitempty"`
	TTI             *uint32 `json:"tti,omitempty"`
	UplinkCapacity  *uint32 `json:"uplinkCapacity,omitempty"`
	DownlinkCapacity *uint32 `json:"downlinkCapacity,omitempty"`
	Congestion      *bool   `json:"congestion,omitempty"`
	ReadBufferSize  *uint32 `json:"readBufferSize,omitempty"`
	WriteBufferSize *uint32 `json:"writeBufferSize,omitempty"`
	Seed            *string `json:"seed,omitempty"`
}

// XHTTPSettings holds transport settings for xHTTP.
type XHTTPSettings struct {
	Host  *string `json:"host,omitempty"`
	Path  *string `json:"path,omitempty"`
	Mode  *string `json:"mode,omitempty"`
	Extra any     `json:"extra,omitempty"`
}

// RealitySettings holds settings for Reality security.
type RealitySettings struct {
	Fingerprint *string `json:"fingerprint,omitempty"`
	ServerName  *string `json:"serverName,omitempty"`
	PublicKey   *string `json:"publicKey,omitempty"`
	ShortId     *string `json:"shortId,omitempty"`
	SpiderX     *string `json:"spiderX,omitempty"`
}

// TCPHeader holds header settings for TCP transport.
type TCPHeader struct {
	Type *string `json:"type,omitempty"`
}

// TCPSettings holds transport settings for TCP.
type TCPSettings struct {
	Header              *TCPHeader `json:"header,omitempty"`
	AcceptProxyProtocol *bool      `json:"acceptProxyProtocol,omitempty"`
}

// WsSettings holds transport settings for WebSocket.
type WsSettings struct {
	Path                *string `json:"path,omitempty"`
	Host                *string `json:"Host,omitempty"`
	AcceptProxyProtocol *bool   `json:"acceptProxyProtocol,omitempty"`
}

// HttpUpgradeSettings holds transport settings for HTTPUpgrade.
type HttpUpgradeSettings struct {
	Host *string `json:"host,omitempty"`
	Path *string `json:"path,omitempty"`
}

// TLSSettings holds TLS security settings.
type TLSSettings struct {
	ALPN                     []string `json:"alpn,omitempty"`
	AllowInsecure            bool     `json:"allowInsecure"`
	ServerName               *string  `json:"serverName,omitempty"`
	EnableSessionResumption  *bool    `json:"enableSessionResumption,omitempty"`
	DisableSystemRoot        *bool    `json:"disableSystemRoot,omitempty"`
	MinVersion               *string  `json:"minVersion,omitempty"`
	MaxVersion               *string  `json:"maxVersion,omitempty"`
	CipherSuites             *string  `json:"cipherSuites,omitempty"`
	PreferServerCipherSuites *bool    `json:"preferServerCipherSuites,omitempty"`
	Fingerprint              *string  `json:"fingerprint,omitempty"`
	RejectUnknownSni         *bool    `json:"rejectUnknownSni,omitempty"`
}

// HysteriaSettings holds stream settings for native Xray hysteria.
type HysteriaSettings struct {
	Version        uint32  `json:"version"`
	Auth           string  `json:"auth"`
	UDPIdleTimeout *uint32 `json:"udpIdleTimeout,omitempty"`
}

// QuicParamsSettings holds brutal bandwidth parameters.
type QuicParamsSettings struct {
	BrutalUp   *string `json:"brutalUp,omitempty"`
	BrutalDown *string `json:"brutalDown,omitempty"`
}

// FinalmaskSettings holds obfuscation and port-hopping settings.
type FinalmaskSettings struct {
	QuicParams *QuicParamsSettings `json:"quicParams,omitempty"`
	UDP        []any               `json:"udp,omitempty"`
}

// StreamSettings describes transport, security, and protocol parameters.
type StreamSettings struct {
	Network             *string              `json:"network,omitempty"`
	Security            *string              `json:"security,omitempty"`
	TLSSettings         *TLSSettings         `json:"tlsSettings,omitempty"`
	WsSettings          *WsSettings          `json:"wsSettings,omitempty"`
	TCPSettings         *TCPSettings         `json:"tcpSettings,omitempty"`
	RealitySettings     *RealitySettings     `json:"realitySettings,omitempty"`
	GRPCSettings        *GRPCSettings        `json:"grpcSettings,omitempty"`
	QuicSettings        *QuicSettings        `json:"quicSettings,omitempty"`
	KCPSettings         *KCPSettings         `json:"kcpSettings,omitempty"`
	XHTTPSettings       *XHTTPSettings       `json:"xhttpSettings,omitempty"`
	HttpUpgradeSettings *HttpUpgradeSettings `json:"httpupgradeSettings,omitempty"`
	HysteriaSettings    *HysteriaSettings    `json:"hysteriaSettings,omitempty"`
	Finalmask           *FinalmaskSettings   `json:"finalmask,omitempty"`
}

// RawData is an intermediate container for all parsed URI fields.
type RawData struct {
	Remarks       string
	Security      *string
	VnextSecurity *string
	SNI           *string
	FP            *string
	PBK           *string
	SID           *string
	Type          *string
	Flow          *string
	Path          *string
	Encryption    *string
	HeaderType    *string
	Host          *string
	Seed          *string
	QuicSecurity  *string
	Key           *string
	Mode          *string
	ServiceName   *string
	Authority     *string
	SLPN          *string
	SPX           *string
	ALPN          *string
	Extra         *string
	AllowInsecure *string
	UUID          *string
	Address       *string
	Port          *uint16
	ServerMethod  *string
	Username      *string
	Obfs          *string
	ObfsPassword  *string
	Up            *string
	Down          *string
	Ports         *string
	FM            *string
}

// ProfileMetadata represents extracted profile metadata.
type ProfileMetadata struct {
	Name     string  `json:"name"`
	Protocol string  `json:"protocol"`
	Host     *string `json:"host,omitempty"`
	Address  *string `json:"address,omitempty"`
	Port     *uint16 `json:"port,omitempty"`
}

// BatchProfileMetadata represents metadata extracted for a batch item.
type BatchProfileMetadata struct {
	URI      string  `json:"uri"`
	Name     string  `json:"name"`
	Protocol string  `json:"protocol"`
	Host     *string `json:"host,omitempty"`
	Address  *string `json:"address,omitempty"`
	Port     *uint16 `json:"port,omitempty"`
}

// Hysteria2 client YAML configuration structures:

type Hysteria2ClientTLS struct {
	SNI      *string  `yaml:"sni,omitempty"`
	Insecure *bool    `yaml:"insecure,omitempty"`
	ALPN     []string `yaml:"alpn,omitempty"`
}

type Hysteria2ClientSalamander struct {
	Password *string `yaml:"password,omitempty"`
}

type Hysteria2ClientObfs struct {
	Type       *string                    `yaml:"type,omitempty"`
	Salamander *Hysteria2ClientSalamander `yaml:"salamander,omitempty"`
}

type Hysteria2ClientBandwidth struct {
	Up   *string `yaml:"up,omitempty"`
	Down *string `yaml:"down,omitempty"`
}

type Hysteria2ClientListen struct {
	Listen string `yaml:"listen"`
}

type Hysteria2ClientConfig struct {
	Server    string                    `yaml:"server"`
	Auth      string                    `yaml:"auth"`
	TLS       *Hysteria2ClientTLS       `yaml:"tls,omitempty"`
	Obfs      *Hysteria2ClientObfs      `yaml:"obfs,omitempty"`
	Bandwidth *Hysteria2ClientBandwidth `yaml:"bandwidth,omitempty"`
	Ports     *string                   `yaml:"ports,omitempty"`
	Socks5    Hysteria2ClientListen     `yaml:"socks5"`
	HTTP      *Hysteria2ClientListen    `yaml:"http,omitempty"`
}
