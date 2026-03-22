# ProvideAnonDBStore (function)

# Slice 1: ProvideAnonDBStore (function, modified)
## Anchor: `ProvideAnonDBStore` in pkg/services/anonymous/anonimpl/anonstore/database.go:52-54
risk=Critical(1.54) blast=6 deps=4

## Detector Findings
- **[HIGH] arity-change-with-callers**: `ProvideAnonDBStore` — Public callable `ProvideAnonDBStore` changed arity (1 -> 2) with 4 caller(s)
  Evidence: `arity 1 -> 2`
- **[HIGH] removed-guard**: `Authenticate` — Guard/assertion removed: `if err := recover(); err != nil {` — safety check may be lost
  Evidence: `if err := recover(); err != nil {`
- **[HIGH] nil-check-missing**: `CreateOrUpdateDevice` — Error return value not checked — may use nil/zero value from failed call
  Evidence: `_, err := dbSession.Exec(args...)`

## Code
### `ProvideAnonDBStore` (function, modified) in database.go:52-54
Called by: TestIntegrationAnonStore_DeleteDevicesOlderThan (database_test.go), TestIntegrationBeyondDeviceLimit (database_test.go), TestIntegrationAnonStore_DeleteDevice (database_test.go)
```
func ProvideAnonDBStore(sqlStore db.DB, deviceLimit int64) *AnonDBStore {
	return &AnonDBStore{sqlStore: sqlStore, log: log.New("anonstore"), deviceLimit: deviceLimit}
}
```

### `ProvideAnonymousDeviceService` (function, modified) in impl.go:36-65
Called by: TestIntegrationDeviceService_tag (impl_test.go), TestIntegrationAnonDeviceService_localCacheSafety (impl_test.go)
Calls: ProvideAnonDBStore
```
func ProvideAnonymousDeviceService(usageStats usagestats.Service, authBroker authn.Service,
	sqlStore db.DB, cfg *setting.Cfg, orgService org.Service,
	serverLockService *serverlock.ServerLockService, accesscontrol accesscontrol.AccessControl, routeRegister routing.RouteRegister,
) *AnonDeviceService {
	a := &AnonDeviceService{
		log:        log.New("anonymous-session-service"),
		localCache: localcache.New(29*time.Minute, 15*time.Minute),
		anonStore:  anonstore.ProvideAnonDBStore(sqlStore, cfg.AnonymousDeviceLimit),
		serverLock: serverLockService,
	}

	usageStats.RegisterMetricsFunc(a.usageStatFn)

	anonClient := &Anonymous{
		cfg:               cfg,
		log:               log.New("authn.anonymous"),
		orgService:        orgService,
		anonDeviceService: a,
	}

	if anonClient.cfg.AnonymousEnabled {
		authBroker.RegisterClient(anonClient)
		authBroker.RegisterPostLoginHook(a.untagDevice, 100)
	}

	anonAPI := api.NewAnonDeviceServiceAPI(cfg, a.anonStore, accesscontrol, routeRegister)
	anonAPI.RegisterAPIEndpoints()

	return a
}
```

### `Authenticate` (method, modified) in client.go:30-59
Called by: TestAnonymous_Authenticate (client_test.go), TestService_Authenticate (service_test.go), TestService_OrgID (service_test.go)
Calls: TagDevice
```
func (a *Anonymous) Authenticate(ctx context.Context, r *authn.Request) (*authn.Identity, error) {
	o, err := a.orgService.GetByName(ctx, &org.GetOrgByNameQuery{Name: a.cfg.AnonymousOrgName})
	if err != nil {
		a.log.FromContext(ctx).Error("Failed to find organization", "name", a.cfg.AnonymousOrgName, "error", err)
		return nil, err
	}

	httpReqCopy := &http.Request{}
	if r.HTTPRequest != nil && r.HTTPRequest.Header != nil {
		// avoid r.HTTPRequest.Clone(context.Background()) as we do not require a full clone
		httpReqCopy.Header = r.HTTPRequest.Header.Clone()
		httpReqCopy.RemoteAddr = r.HTTPRequest.RemoteAddr
	}

	if err := a.anonDeviceService.TagDevice(ctx, httpReqCopy, anonymous.AnonDeviceUI); err != nil {
		if errors.Is(err, anonstore.ErrDeviceLimitReached) {
			return nil, err
		}

		a.log.Warn("Failed to tag anonymous session", "error", err)
	}

	return &authn.Identity{
		ID:           authn.AnonymousNamespaceID,
		OrgID:        o.ID,
		OrgName:      o.Name,
		OrgRoles:     map[int64]org.RoleType{o.ID: org.RoleType(a.cfg.AnonymousOrgRole)},
		ClientParams: authn.ClientParams{SyncPermissions: true},
	}, nil
}
```

### `TagDevice` (method, modified) in impl.go:118-151
Called by: Authenticate (client.go), TestIntegrationDeviceService_tag (impl_test.go), TestIntegrationAnonDeviceService_localCacheSafety (impl_test.go)
```
func (a *AnonDeviceService) TagDevice(ctx context.Context, httpReq *http.Request, kind anonymous.DeviceKind) error {
	deviceID := httpReq.Header.Get(deviceIDHeader)
	if deviceID == "" {
		return nil
	}

	addr := web.RemoteAddr(httpReq)
	ip, err := network.GetIPFromAddress(addr)
	if err != nil {
		a.log.Debug("Failed to parse ip from address", "addr", addr)
		return nil
	}

	clientIPStr := ip.String()
	if len(ip) == 0 {
		clientIPStr = ""
	}

	taggedDevice := &anonstore.Device{
		DeviceID:  deviceID,
		ClientIP:  clientIPStr,
		UserAgent: httpReq.UserAgent(),
		CreatedAt: time.Now(),
		UpdatedAt: time.Now(),
	}

	err = a.tagDeviceUI(ctx, httpReq, taggedDevice)
	if err != nil {
		a.log.Debug("Failed to tag device for UI", "error", err)
		return err
	}

	return nil
}
```

### `CreateOrUpdateDevice` (method, modified) in database.go:105-156
Called by: TestIntegrationAnonStore_DeleteDevicesOlderThan (database_test.go), TestIntegrationBeyondDeviceLimit (database_test.go), TestIntegrationAnonStore_DeleteDevice (database_test.go)
Calls: updateDevice
```
func (s *AnonDBStore) CreateOrUpdateDevice(ctx context.Context, device *Device) error {
	var query string

	// if device limit is reached, only update devices
	if s.deviceLimit > 0 {
		count, err := s.CountDevices(ctx, time.Now().UTC().Add(-anonymousDeviceExpiration), time.Now().UTC().Add(time.Minute))
		if err != nil {
			return err
		}

		if count >= s.deviceLimit {
			return s.updateDevice(ctx, device)
		}
	}

	args := []any{device.DeviceID, device.ClientIP, device.UserAgent,
		device.CreatedAt.UTC(), device.UpdatedAt.UTC()}
	switch s.sqlStore.GetDBType() {
	case migrator.Postgres:
		query = `INSERT INTO anon_device (device_id, client_ip, user_agent, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5)
ON CONFLICT (device_id) DO UPDATE SET
client_ip = $2,
user_agent = $3,
updated_at = $5
RETURNING id`
	case migrator.MySQL:
		query = `INSERT INTO anon_device (device_id, client_ip, user_agent, created_at, updated_at)
VALUES (?, ?, ?, ?, ?)
ON DUPLICATE KEY UPDATE
client_ip = VALUES(client_ip),
user_agent = VALUES(user_agent),
updated_at = VALUES(updated_at)`
	case migrator.SQLite:
		query = `INSERT INTO anon_device (device_id, client_ip, user_agent, created_at, updated_at)
VALUES (
... (truncated)
```

### `updateDevice` (method, added) in database.go:73-103
Called by: CreateOrUpdateDevice (database.go)
```
func (s *AnonDBStore) updateDevice(ctx context.Context, device *Device) error {
	const query = `UPDATE anon_device SET
client_ip = ?,
user_agent = ?,
updated_at = ?
WHERE device_id = ? AND updated_at BETWEEN ? AND ?`

	args := []interface{}{device.ClientIP, device.UserAgent, device.UpdatedAt.UTC(), device.DeviceID,
		device.UpdatedAt.UTC().Add(-anonymousDeviceExpiration), device.UpdatedAt.UTC().Add(time.Minute),
	}
	err := s.sqlStore.WithDbSession(ctx, func(dbSession *sqlstore.DBSession) error {
		args = append([]interface{}{query}, args...)
		result, err := dbSession.Exec(args...)
		if err != nil {
			return err
		}

		rowsAffected, err := result.RowsAffected()
		if err != nil {
			return err
		}

		if rowsAffected == 0 {
			return ErrDeviceLimitReached
		}

		return nil
	})

	return err
}
```

## Hypotheses to verify
- Guard removed: `if err := recover(); err != nil {`. What did it protect? Is the scenario still handled?
- `CreateOrUpdateDevice` reads a count then writes — is this atomic? Without a transaction, concurrent requests can both pass the count check (TOCTOU race).
- `updateDevice` returns an error when rowsAffected==0, but zero rows could mean the record doesn't exist OR doesn't match the WHERE filter — not necessarily the named error.
- `updateDevice` uses a time-window filter. Check if the window aligns with cleanup/expiration — records outside the window but not yet deleted will be missed.


---

# FetchTeamMemberships (method)

# Slice 2: FetchTeamMemberships (method, added)
## Anchor: `FetchTeamMemberships` in pkg/login/social/generic_oauth.go:452-467
risk=Critical(1.46) blast=11 deps=1

## Detector Findings
- **[HIGH] nil-check-missing**: `FetchTeamMemberships` — Error return value not checked — may use nil/zero value from failed call
  Evidence: `ids, err = s.fetchTeamMembershipsFromDeprecatedTeamsUrl(ctx, client)`

## Code
### `FetchTeamMemberships` (method, added) in generic_oauth.go:452-467
Called by: IsTeamMember (generic_oauth.go)
Calls: Client, fetchTeamMembershipsFromDeprecatedTeamsUrl, fetchTeamMembershipsFromTeamsUrl
```
func (s *SocialGenericOAuth) FetchTeamMemberships(ctx context.Context, client *http.Client) ([]string, error) {
	var err error
	var ids []string

	if s.teamsUrl == "" {
		ids, err = s.fetchTeamMembershipsFromDeprecatedTeamsUrl(ctx, client)
	} else {
		ids, err = s.fetchTeamMembershipsFromTeamsUrl(ctx, client)
	}

	if err == nil {
		s.log.Debug("Received team memberships", "ids", ids)
	}

	return ids, err
}
```

### `fetchTeamMembershipsFromDeprecatedTeamsUrl` (method, added) in generic_oauth.go:469-496
Called by: FetchTeamMemberships (generic_oauth.go)
Calls: Client, httpGet
```
func (s *SocialGenericOAuth) fetchTeamMembershipsFromDeprecatedTeamsUrl(ctx context.Context, client *http.Client) ([]string, error) {
	var ids []string

	type Record struct {
		Id int `json:"id"`
	}

	response, err := s.httpGet(ctx, client, fmt.Sprintf(s.apiUrl+"/teams"))
	if err != nil {
		s.log.Error("Error getting team memberships", "url", s.apiUrl+"/teams", "error", err)
		return []string{}, err
	}

	var records []Record

	err = json.Unmarshal(response.Body, &records)
	if err != nil {
		s.log.Error("Error decoding team memberships response", "raw_json", string(response.Body), "error", err)
		return []string{}, err
	}

	ids = make([]string, len(records))
	for i, record := range records {
		ids[i] = strconv.Itoa(record.Id)
	}

	return ids, nil
}
```

### `fetchTeamMembershipsFromTeamsUrl` (method, added) in generic_oauth.go:498-510
Called by: FetchTeamMemberships (generic_oauth.go)
Calls: Client, httpGet, searchJSONForStringArrayAttr
```
func (s *SocialGenericOAuth) fetchTeamMembershipsFromTeamsUrl(ctx context.Context, client *http.Client) ([]string, error) {
	if s.teamIdsAttributePath == "" {
		return []string{}, nil
	}

	response, err := s.httpGet(ctx, client, fmt.Sprintf(s.teamsUrl))
	if err != nil {
		s.log.Error("Error getting team memberships", "url", s.teamsUrl, "error", err)
		return nil, err
	}

	return s.searchJSONForStringArrayAttr(s.teamIdsAttributePath, response.Body)
}
```

### `IsTeamMember` (method, added) in generic_oauth.go:93-112
Called by: UserInfo (generic_oauth.go), addTeamMember (team_members.go), updateTeamMember (team_members.go)
Calls: Client, FetchTeamMemberships
```
func (s *SocialGenericOAuth) IsTeamMember(ctx context.Context, client *http.Client) bool {
	if len(s.teamIds) == 0 {
		return true
	}

	teamMemberships, err := s.FetchTeamMemberships(ctx, client)
	if err != nil {
		return false
	}

	for _, teamId := range s.teamIds {
		for _, membershipId := range teamMemberships {
			if teamId == membershipId {
				return true
			}
		}
	}

	return false
}
```

### `httpGet` (method, added) in common.go:60-91
Called by: retrieveGeneralJWKS (azuread_jwks.go), retrieveSpecificJWKS (azuread_jwks.go), extractFromAPI (generic_oauth.go)
Calls: Client
```
func (s *SocialBase) httpGet(ctx context.Context, client *http.Client, url string) (*httpGetResponse, error) {
	req, errReq := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if errReq != nil {
		return nil, errReq
	}

	r, errDo := client.Do(req)
	if errDo != nil {
		return nil, errDo
	}

	defer func() {
		if err := r.Body.Close(); err != nil {
			s.log.Warn("Failed to close response body", "err", err)
		}
	}()

	body, errRead := io.ReadAll(r.Body)
	if errRead != nil {
		return nil, errRead
	}

	response := &httpGetResponse{body, r.Header}

	if r.StatusCode >= 300 {
		return nil, fmt.Errorf("unsuccessful response status code %d: %s", r.StatusCode, string(response.Body))
	}

	s.log.Debug("HTTP GET", "url", url, "status", r.Status, "response_body", string(response.Body))

	return response, nil
}
```

### `searchJSONForStringArrayAttr` (method, added) in common.go:129-148
Called by: extractGroups (generic_oauth.go), fetchTeamMembershipsFromTeamsUrl (generic_oauth.go), TestSearchJSONForGroups (generic_oauth_test.go)
Calls: searchJSONForAttr
```
func (s *SocialBase) searchJSONForStringArrayAttr(attributePath string, data []byte) ([]string, error) {
	val, err := s.searchJSONForAttr(attributePath, data)
	if err != nil {
		return []string{}, err
	}

	ifArr, ok := val.([]any)
	if !ok {
		return []string{}, nil
	}

	result := []string{}
	for _, v := range ifArr {
		if strVal, ok := v.(string); ok {
			result = append(result, strVal)
		}
	}

	return result, nil
}
```

### `UserInfo` (method, added) in generic_oauth.go:154-250
Calls: Client, extractFromToken, extractFromAPI
```
func (s *SocialGenericOAuth) UserInfo(ctx context.Context, client *http.Client, token *oauth2.Token) (*BasicUserInfo, error) {
	s.log.Debug("Getting user info")
	toCheck := make([]*UserInfoJson, 0, 2)

	if tokenData := s.extractFromToken(token); tokenData != nil {
		toCheck = append(toCheck, tokenData)
	}
	if apiData := s.extractFromAPI(ctx, client); apiData != nil {
		toCheck = append(toCheck, apiData)
	}

	userInfo := &BasicUserInfo{}
	for _, data := range toCheck {
		s.log.Debug("Processing external user info", "source", data.source, "data", data)

		if userInfo.Id == "" {
			userInfo.Id = data.Sub
		}

		if userInfo.Name == "" {
			userInfo.Name = s.extractUserName(data)
		}

		if userInfo.Login == "" {
			userInfo.Login = s.extractLogin(data)
		}

		if userInfo.Email == "" {
			userInfo.Email = s.extractEmail(data)
			if userInfo.Email != "" {
				s.log.Debug("Set user info email from extracted email", "email", userInfo.Email)
			}
		}

		if userInfo.Role == "" && !s.skipOrgRoleSync {
			role, grafanaAdmin, err := s.extractRoleAndAdminOptional(data.rawJSON, []string{})
			if err != nil {
				s.log.Warn("Failed to extract role", "err", err)
			} else {
				userInfo.Role = role

... (truncated)
```

### `retrieveGeneralJWKS` (method, added) in azuread_jwks.go:56-74
Called by: validateIDTokenSignature (azuread_oauth.go)
Calls: Client, httpGet, getCacheExpiration
```
func (s *SocialAzureAD) retrieveGeneralJWKS(ctx context.Context, client *http.Client, authURL string) (*keySetJWKS, time.Duration, error) {
	keysetURL := strings.Replace(authURL, "/oauth2/v2.0/authorize", "/discovery/v2.0/keys", 1)

	resp, err := s.httpGet(ctx, client, keysetURL)
	if err != nil {
		return nil, 0, err
	}

	bytesReader := bytes.NewReader(resp.Body)
	var jwks keySetJWKS
	if err := json.NewDecoder(bytesReader).Decode(&jwks); err != nil {
		return nil, 0, err
	}

	cacheExpiration := getCacheExpiration(resp.Headers.Get("cache-control"))
	s.log.Debug("Retrieved general key set", "url", keysetURL, "cacheExpiration", cacheExpiration)

	return &jwks, cacheExpiration, nil
}
```

### `retrieveSpecificJWKS` (method, added) in azuread_jwks.go:76-94
Called by: validateIDTokenSignature (azuread_oauth.go)
Calls: Client, httpGet, getCacheExpiration
```
func (s *SocialAzureAD) retrieveSpecificJWKS(ctx context.Context, client *http.Client, authURL string) (*keySetJWKS, time.Duration, error) {
	keysetURL := strings.Replace(authURL, "/oauth2/v2.0/authorize", "/discovery/v2.0/keys", 1) + "?appid=" + s.ClientID

	resp, err := s.httpGet(ctx, client, keysetURL)
	if err != nil {
		return nil, 0, err
	}

	bytesReader := bytes.NewReader(resp.Body)
	var jwks keySetJWKS
	if err := json.NewDecoder(bytesReader).Decode(&jwks); err != nil {
		return nil, 0, err
	}

	cacheExpiration := getCacheExpiration(resp.Headers.Get("cache-control"))
	s.log.Debug("Retrieved specific key set", "url", keysetURL, "cacheExpiration", cacheExpiration)

	return &jwks, cacheExpiration, nil
}
```

### `extractFromAPI` (method, added) in generic_oauth.go:289-314
Called by: UserInfo (generic_oauth.go)
Calls: Client, httpGet
```
func (s *SocialGenericOAuth) extractFromAPI(ctx context.Context, client *http.Client) *UserInfoJson {
	s.log.Debug("Getting user info from API")
	if s.apiUrl == "" {
		s.log.Debug("No api url configured")
		return nil
	}

	rawUserInfoResponse, err := s.httpGet(ctx, client, s.apiUrl)
	if err != nil {
		s.log.Debug("Error getting user info from API", "url", s.apiUrl, "error", err)
		return nil
	}

	rawJSON := rawUserInfoResponse.Body

	var data UserInfoJson
	if err := json.Unmarshal(rawJSON, &data); err != nil {
		s.log.Error("Error decoding user info response", "raw_json", rawJSON, "error", err)
		return nil
	}

	data.rawJSON = rawJSON
	data.source = "API"
	s.log.Debug("Received user info response from API", "raw_json", string(rawJSON), "data", data.String())
	return &data
}
```

### `FetchPrivateEmail` (method, added) in generic_oauth.go:405-450
Called by: UserInfo (generic_oauth.go)
Calls: Client, httpGet
```
func (s *SocialGenericOAuth) FetchPrivateEmail(ctx context.Context, client *http.Client) (string, error) {
	type Record struct {
		Email       string `json:"email"`
		Primary     bool   `json:"primary"`
		IsPrimary   bool   `json:"is_primary"`
		Verified    bool   `json:"verified"`
		IsConfirmed bool   `json:"is_confirmed"`
	}

	response, err := s.httpGet(ctx, client, fmt.Sprintf(s.apiUrl+"/emails"))
	if err != nil {
		s.log.Error("Error getting email address", "url", s.apiUrl+"/emails", "error", err)
		return "", fmt.Errorf("%v: %w", "Error getting email address", err)
	}

	var records []Record

	err = json.Unmarshal(response.Body, &records)
	if err != nil {
		var data struct {
			Values []Record `json:"values"`
		}

		err = json.Unmarshal(response.Body, &data)
		if err != nil {
			s.log.Error("Error decoding email addresses response", "raw_json", string(response.Body), "error", err)
			return "", fmt.Errorf("%v: %w", "Error decoding email addresses response", err)
		}

		records = data.Values
	}

	s.log.Debug("Received email addresses", "emails", records)

	var email = ""
	for _, record := range records {
		if record.Primary || record.IsPrimary {
			email = record.Email
			break

... (truncated)
```

### `FetchOrganizations` (method, added) in generic_oauth.go:512-539
Called by: IsOrganizationMember (generic_oauth.go)
Calls: Client, httpGet
```
func (s *SocialGenericOAuth) FetchOrganizations(ctx context.Context, client *http.Client) ([]string, bool) {
	type Record struct {
		Login string `json:"login"`
	}

	response, err := s.httpGet(ctx, client, fmt.Sprintf(s.apiUrl+"/orgs"))
	if err != nil {
		s.log.Error("Error getting organizations", "url", s.apiUrl+"/orgs", "error", err)
		return nil, false
	}

	var records []Record

	err = json.Unmarshal(response.Body, &records)
	if err != nil {
		s.log.Error("Error decoding organization response", "response", string(response.Body), "error", err)
		return nil, false
	}

	var logins = make([]string, len(records))
	for i, record := range records {
		logins[i] = record.Login
	}

	s.log.Debug("Received organizations", "logins", logins)

	return logins, true
}
```

### `FetchPrivateEmail` (method, added) in github_oauth.go:119-146
Called by: UserInfo (github_oauth.go)
Calls: Client, httpGet
```
func (s *SocialGithub) FetchPrivateEmail(ctx context.Context, client *http.Client) (string, error) {
	type Record struct {
		Email    string `json:"email"`
		Primary  bool   `json:"primary"`
		Verified bool   `json:"verified"`
	}

	response, err := s.httpGet(ctx, client, fmt.Sprintf(s.apiUrl+"/emails"))
	if err != nil {
		return "", fmt.Errorf("Error getting email address: %s", err)
	}

	var records []Record

	err = json.Unmarshal(response.Body, &records)
	if err != nil {
		return "", fmt.Errorf("Error getting email address: %s", err)
	}

	var email = ""
	for _, record := range records {
		if record.Primary {
			email = record.Email
		}
	}

	return email, nil
}
```

## Hypotheses to verify


---

# queryFlameGraph (variable)

# Slice 3: queryFlameGraph (variable, modified)
## Anchor: `queryFlameGraph` in public/app/features/explore/TraceView/components/TraceTimelineViewer/SpanDetail/SpanFlameGraph.tsx:75-111
risk=Critical(1.28) blast=5 deps=1

## Detector Findings
- **[HIGH] removed-guard**: `queryFlameGraph` — Guard/assertion removed: `if (traceToProfilesOptions.customQuery && traceToProfilesOptions.query) {` — safety check may be lost
  Evidence: `if (traceToProfilesOptions.customQuery && traceToProfilesOptions.query) {`
- **[HIGH] removed-guard**: `SpanFlameGraph` — Guard/assertion removed: `if (traceToProfilesOptions.customQuery && traceToProfilesOptions.query) {` — safety check may be lost
  Evidence: `if (traceToProfilesOptions.customQuery && traceToProfilesOptions.query) {`

## Code
### `queryFlameGraph` (variable, modified) in SpanFlameGraph.tsx:75-111
Called by: SpanFlameGraph (SpanFlameGraph.tsx)
```
const queryFlameGraph = useCallback(
    async (
      profilesDataSourceSettings: DataSourceInstanceSettings<DataSourceJsonData>,
      traceToProfilesOptions: TraceToProfilesOptions
    ) => {
      const request = {
        requestId: 'span-flamegraph-requestId',
        interval: '2s',
        intervalMs: 2000,
        range: getTimeRangeForProfile(),
        scopedVars: {},
        app: CoreApp.Unknown,
        timezone: timeZone,
        startTime: span.startTime,
        targets: [
          {
            labelSelector: '{}',
            groupBy: [],
            profileTypeId: traceToProfilesOptions.profileTypeId ?? '',
            queryType: 'profile' as PyroscopeQueryType,
            spanSelector: [profileTagValue],
            refId: 'span-flamegraph-refId',
            datasource: {
              type: profilesDataSourceSettings.type,
              uid: profilesDataSourceSettings.uid,
            },
          },
        ],
      };
      const flameGraph = await getFlameGraphData(request, profilesDataSourceSettings.uid);

      if (flameGraph && flameGraph.length > 0) {
        setTraceFlameGraphs({ ...traceFlameGraphs, [profileTagValue]: flameGraph });
      }
    },
 
... (truncated)
```

### `SpanFlameGraph` (function, modified) in SpanFlameGraph.tsx:38-155
Called by: SpanDetail (index.tsx)
Calls: queryFlameGraph
```
function SpanFlameGraph(props: SpanFlameGraphProps) {
  const { span, traceToProfilesOptions, timeZone, traceFlameGraphs, setTraceFlameGraphs, setRedrawListView } = props;
  const [sizeRef, { height: containerHeight }] = useMeasure<HTMLDivElement>();
  const styles = useStyles2(getStyles);

  const profileTag = span.tags.filter((tag) => tag.key === pyroscopeProfileIdTagKey);
  const profileTagValue = profileTag.length > 0 ? profileTag[0].value : undefined;

  const getTimeRangeForProfile = useCallback(() => {
    const spanStartMs = Math.floor(span.startTime / 1000) - 30000;
    const spanEndMs = (span.startTime + span.duration) / 1000 + 30000;
    const to = dateTime(spanEndMs);
    const from = dateTime(spanStartMs);

    return {
      from,
      to,
      raw: {
        from,
        to,
      },
    };
  }, [span.duration, span.startTime]);

  const getFlameGraphData = async (request: DataQueryRequest<Query>, datasourceUid: string) => {
    const ds = await getDatasourceSrv().get(datasourceUid);
    if (ds instanceof PyroscopeDataSource) {
      const result = await lastValueFrom(ds.query(request));
      const frame = result.data.find((x: DataFrame) => {
        return x.nam
... (truncated)
```

## Hypotheses to verify
- Guard removed: `if (traceToProfilesOptions.customQuery && traceToProfilesOptions.query) {`. What did it protect? Is the scenario still handled?


---

# searchRole (method)

# Slice 4: searchRole (method, added)
## Anchor: `searchRole` in pkg/login/social/social.go:287-301
risk=Critical(1.16) blast=3 deps=1

## Detector Findings
- **[HIGH] nil-check-missing**: `extractRoleAndAdmin` — Error return value not checked — may use nil/zero value from failed call
  Evidence: `role, gAdmin, err := s.extractRoleAndAdminOptional(rawJSON, groups)`
- **[HIGH] nil-check-missing**: `searchRole` — Error return value not checked — may use nil/zero value from failed call
  Evidence: `role, err := s.searchJSONForStringAttr(s.roleAttributePath, rawJSON)`

## Code
### `searchRole` (method, added) in social.go:287-301
Called by: extractRoleAndAdminOptional (social.go)
Calls: searchJSONForStringAttr, getRoleFromSearch
```
func (s *SocialBase) searchRole(rawJSON []byte, groups []string) (org.RoleType, bool) {
	role, err := s.searchJSONForStringAttr(s.roleAttributePath, rawJSON)
	if err == nil && role != "" {
		return getRoleFromSearch(role)
	}

	if groupBytes, err := json.Marshal(groupStruct{groups}); err == nil {
		role, err := s.searchJSONForStringAttr(s.roleAttributePath, groupBytes)
		if err == nil && role != "" {
			return getRoleFromSearch(role)
		}
	}

	return "", false
}
```

### `searchJSONForStringAttr` (method, added) in common.go:115-127
Called by: extractEmail (generic_oauth.go), extractLogin (generic_oauth.go), extractUserName (generic_oauth.go)
Calls: searchJSONForAttr
```
func (s *SocialBase) searchJSONForStringAttr(attributePath string, data []byte) (string, error) {
	val, err := s.searchJSONForAttr(attributePath, data)
	if err != nil {
		return "", err
	}

	strVal, ok := val.(string)
	if ok {
		return strVal, nil
	}

	return "", nil
}
```

### `getRoleFromSearch` (function, added) in social.go:317-323
Called by: searchRole (social.go)
```
func getRoleFromSearch(role string) (org.RoleType, bool) {
	if strings.EqualFold(role, RoleGrafanaAdmin) {
		return org.RoleAdmin, true
	}

	return org.RoleType(cases.Title(language.Und).String(role)), false
}
```

### `extractRoleAndAdminOptional` (method, added) in social.go:257-276
Called by: UserInfo (generic_oauth.go), extractRoleAndAdmin (social.go)
Calls: searchRole
```
func (s *SocialBase) extractRoleAndAdminOptional(rawJSON []byte, groups []string) (org.RoleType, bool, error) {
	if s.roleAttributePath == "" {
		if s.roleAttributeStrict {
			return "", false, errRoleAttributePathNotSet.Errorf("role_attribute_path not set and role_attribute_strict is set")
		}
		return "", false, nil
	}

	if role, gAdmin := s.searchRole(rawJSON, groups); role.IsValid() {
		return role, gAdmin, nil
	} else if role != "" {
		return "", false, errInvalidRole.Errorf("invalid role: %s", role)
	}

	if s.roleAttributeStrict {
		return "", false, errRoleAttributeStrictViolation.Errorf("idP did not return a role attribute, but role_attribute_strict is set")
	}

	return "", false, nil
}
```

### `searchJSONForAttr` (method, added) in common.go:93-113
Called by: searchJSONForStringAttr (common.go), searchJSONForStringArrayAttr (common.go)
```
func (s *SocialBase) searchJSONForAttr(attributePath string, data []byte) (any, error) {
	if attributePath == "" {
		return "", errors.New("no attribute path specified")
	}

	if len(data) == 0 {
		return "", errors.New("empty user info JSON response provided")
	}

	var buf any
	if err := json.Unmarshal(data, &buf); err != nil {
		return "", fmt.Errorf("%v: %w", "failed to unmarshal user info JSON response", err)
	}

	val, err := jmespath.Search(attributePath, buf)
	if err != nil {
		return "", fmt.Errorf("failed to search user info JSON response with provided path: %q: %w", attributePath, err)
	}

	return val, nil
}
```

### `extractEmail` (method, added) in generic_oauth.go:316-344
Called by: UserInfo (generic_oauth.go)
Calls: searchJSONForStringAttr
```
func (s *SocialGenericOAuth) extractEmail(data *UserInfoJson) string {
	if data.Email != "" {
		return data.Email
	}

	if s.emailAttributePath != "" {
		email, err := s.searchJSONForStringAttr(s.emailAttributePath, data.rawJSON)
		if err != nil {
			s.log.Error("Failed to search JSON for attribute", "error", err)
		} else if email != "" {
			return email
		}
	}

	emails, ok := data.Attributes[s.emailAttributeName]
	if ok && len(emails) != 0 {
		return emails[0]
	}

	if data.Upn != "" {
		emailAddr, emailErr := mail.ParseAddress(data.Upn)
		if emailErr == nil {
			return emailAddr.Address
		}
		s.log.Debug("Failed to parse e-mail address", "error", emailErr.Error())
	}

	return ""
}
```

### `extractLogin` (method, added) in generic_oauth.go:346-370
Called by: UserInfo (generic_oauth.go)
Calls: searchJSONForStringAttr
```
func (s *SocialGenericOAuth) extractLogin(data *UserInfoJson) string {
	if data.Login != "" {
		s.log.Debug("Setting user info login from login field", "login", data.Login)
		return data.Login
	}

	if s.loginAttributePath != "" {
		s.log.Debug("Searching for login among JSON", "loginAttributePath", s.loginAttributePath)
		login, err := s.searchJSONForStringAttr(s.loginAttributePath, data.rawJSON)
		if err != nil {
			s.log.Error("Failed to search JSON for login attribute", "error", err)
		}

		if login != "" {
			return login
		}
	}

	if data.Username != "" {
		s.log.Debug("Setting user info login from username field", "username", data.Username)
		return data.Username
	}

	return ""
}
```

### `extractUserName` (method, added) in generic_oauth.go:372-395
Called by: UserInfo (generic_oauth.go)
Calls: searchJSONForStringAttr
```
func (s *SocialGenericOAuth) extractUserName(data *UserInfoJson) string {
	if s.nameAttributePath != "" {
		name, err := s.searchJSONForStringAttr(s.nameAttributePath, data.rawJSON)
		if err != nil {
			s.log.Error("Failed to search JSON for attribute", "error", err)
		} else if name != "" {
			s.log.Debug("Setting user info name from nameAttributePath", "nameAttributePath", s.nameAttributePath)
			return name
		}
	}

	if data.Name != "" {
		s.log.Debug("Setting user info name from name field")
		return data.Name
	}

	if data.DisplayName != "" {
		s.log.Debug("Setting user info name from display name field")
		return data.DisplayName
	}

	s.log.Debug("Unable to find user info name")
	return ""
}
```

### `UserInfo` (method, added) in generic_oauth.go:154-250
Calls: Client, extractFromToken, extractFromAPI
```
func (s *SocialGenericOAuth) UserInfo(ctx context.Context, client *http.Client, token *oauth2.Token) (*BasicUserInfo, error) {
	s.log.Debug("Getting user info")
	toCheck := make([]*UserInfoJson, 0, 2)

	if tokenData := s.extractFromToken(token); tokenData != nil {
		toCheck = append(toCheck, tokenData)
	}
	if apiData := s.extractFromAPI(ctx, client); apiData != nil {
		toCheck = append(toCheck, apiData)
	}

	userInfo := &BasicUserInfo{}
	for _, data := range toCheck {
		s.log.Debug("Processing external user info", "source", data.source, "data", data)

		if userInfo.Id == "" {
			userInfo.Id = data.Sub
		}

		if userInfo.Name == "" {
			userInfo.Name = s.extractUserName(data)
		}

		if userInfo.Login == "" {
			userInfo.Login = s.extractLogin(data)
		}

		if userInfo.Email == "" {
			userInfo.Email = s.extractEmail(data)
			if userInfo.Email != "" {
				s.log.Debug("Set user info email from extracted email", "email", userInfo.Email)
			}
		}

		if userInfo.Role == "" && !s.skipOrgRoleSync {
			role, grafanaAdmin, err := s.extractRoleAndAdminOptional(data.rawJSON, []string{})
			if err != nil {
				s.log.Warn("Failed to extract role", "err", err)
			} else {
				userInfo.Role = role

... (truncated)
```

### `extractRoleAndAdmin` (method, added) in social.go:278-285
Calls: extractRoleAndAdminOptional, defaultRole
```
func (s *SocialBase) extractRoleAndAdmin(rawJSON []byte, groups []string) (org.RoleType, bool, error) {
	role, gAdmin, err := s.extractRoleAndAdminOptional(rawJSON, groups)
	if role == "" {
		role = s.defaultRole()
	}

	return role, gAdmin, err
}
```

### `searchJSONForStringArrayAttr` (method, added) in common.go:129-148
Called by: extractGroups (generic_oauth.go), fetchTeamMembershipsFromTeamsUrl (generic_oauth.go), TestSearchJSONForGroups (generic_oauth_test.go)
Calls: searchJSONForAttr
```
func (s *SocialBase) searchJSONForStringArrayAttr(attributePath string, data []byte) ([]string, error) {
	val, err := s.searchJSONForAttr(attributePath, data)
	if err != nil {
		return []string{}, err
	}

	ifArr, ok := val.([]any)
	if !ok {
		return []string{}, nil
	}

	result := []string{}
	for _, v := range ifArr {
		if strVal, ok := v.(string); ok {
			result = append(result, strVal)
		}
	}

	return result, nil
}
```

### `extractFromToken` (method, added) in generic_oauth.go:256-287
Called by: UserInfo (generic_oauth.go), TestPayloadCompression (generic_oauth_test.go), TestSocialGitlab_extractFromToken (gitlab_oauth_test.go)
Calls: retrieveRawIDToken
```
func (s *SocialGenericOAuth) extractFromToken(token *oauth2.Token) *UserInfoJson {
	s.log.Debug("Extracting user info from OAuth token")

	idTokenAttribute := "id_token"
	if s.idTokenAttributeName != "" {
		idTokenAttribute = s.idTokenAttributeName
		s.log.Debug("Using custom id_token attribute name", "attribute_name", idTokenAttribute)
	}

	idToken := token.Extra(idTokenAttribute)
	if idToken == nil {
		s.log.Debug("No id_token found", "token", token)
		return nil
	}

	rawJSON, err := s.retrieveRawIDToken(idToken)
	if err != nil {
		s.log.Warn("Error retrieving id_token", "error", err, "token", fmt.Sprintf("%+v", token))
		return nil
	}

	var data UserInfoJson
	if err := json.Unmarshal(rawJSON, &data); err != nil {
		s.log.Error("Error decoding id_token JSON", "raw_json", string(rawJSON), "error", err)
		return nil
	}

	data.rawJSON = rawJSON
	data.source = "token"
	s.log.Debug("Received id_token", "raw_json", string(data.rawJSON), "data", data.String())
	return &data
}
```

### `extractFromAPI` (method, added) in generic_oauth.go:289-314
Called by: UserInfo (generic_oauth.go)
Calls: Client, httpGet
```
func (s *SocialGenericOAuth) extractFromAPI(ctx context.Context, client *http.Client) *UserInfoJson {
	s.log.Debug("Getting user info from API")
	if s.apiUrl == "" {
		s.log.Debug("No api url configured")
		return nil
	}

	rawUserInfoResponse, err := s.httpGet(ctx, client, s.apiUrl)
	if err != nil {
		s.log.Debug("Error getting user info from API", "url", s.apiUrl, "error", err)
		return nil
	}

	rawJSON := rawUserInfoResponse.Body

	var data UserInfoJson
	if err := json.Unmarshal(rawJSON, &data); err != nil {
		s.log.Error("Error decoding user info response", "raw_json", rawJSON, "error", err)
		return nil
	}

	data.rawJSON = rawJSON
	data.source = "API"
	s.log.Debug("Received user info response from API", "raw_json", string(rawJSON), "data", data.String())
	return &data
}
```

## Hypotheses to verify


---

# RegisterReloadable (method)

# Slice 5: RegisterReloadable (method, modified)
## Anchor: `RegisterReloadable` in pkg/services/ssosettings/ssosettingsimpl/service.go:149-151
risk=Critical(1.11) blast=0 deps=0

## Detector Findings
- **[HIGH] removed-guard**: `RegisterReloadable` — Guard/assertion removed: `if s.reloadables == nil {` — safety check may be lost
  Evidence: `if s.reloadables == nil {`

## Code
### `RegisterReloadable` (method, modified) in service.go:149-151
```
func (s *SSOSettingsService) RegisterReloadable(ctx context.Context, provider string, reloadable ssosettings.Reloadable) {
	panic("not implemented") // TODO: Implement
}
```

## Hypotheses to verify
- Guard removed: `if s.reloadables == nil {`. What did it protect? Is the scenario still handled?


---

# CreateOAuthInfoFromKeyValues (function)

# Slice 6: CreateOAuthInfoFromKeyValues (function, added)
## Anchor: `CreateOAuthInfoFromKeyValues` in pkg/login/social/common.go:209-246
risk=Critical(0.92) blast=32 deps=9

## Detector Findings
- **[HIGH] nil-check-missing**: `CreateOAuthInfoFromKeyValues` — Error return value not checked — may use nil/zero value from failed call
  Evidence: `decoder, err := mapstructure.NewDecoder(&mapstructure.DecoderConfig{`

## Code
### `CreateOAuthInfoFromKeyValues` (function, added) in common.go:209-246
Called by: NewAzureADProvider (azuread_oauth.go), TestMapping_IniSectionOAuthInfo (commont_test.go), NewGenericOAuthProvider (generic_oauth.go)
```
func CreateOAuthInfoFromKeyValues(settingsKV map[string]any) (*OAuthInfo, error) {
	emptyStrToSliceDecodeHook := func(from reflect.Type, to reflect.Type, data any) (any, error) {
		if from.Kind() == reflect.String && to.Kind() == reflect.Slice {
			strData, ok := data.(string)
			if !ok {
				return nil, fmt.Errorf("failed to convert %v to string", data)
			}

			if strData == "" {
				return []string{}, nil
			}
			return util.SplitString(strData), nil
		}
		return data, nil
	}

	var oauthInfo OAuthInfo
	decoder, err := mapstructure.NewDecoder(&mapstructure.DecoderConfig{
		DecodeHook:       emptyStrToSliceDecodeHook,
		Result:           &oauthInfo,
		WeaklyTypedInput: true,
	})

	if err != nil {
		return nil, err
	}

	err = decoder.Decode(settingsKV)
	if err != nil {
		return nil, err
	}

	if oauthInfo.EmptyScopes {
		oauthInfo.Scopes = []string{}
	}

	return &oauthInfo, err
}
```

### `NewAzureADProvider` (function, added) in azuread_oauth.go:75-97
Called by: TestSocialAzureAD_UserInfo (azuread_oauth_test.go), TestSocialAzureAD_SkipOrgRole (azuread_oauth_test.go), TestSocialAzureAD_InitializeExtraFields (azuread_oauth_test.go)
Calls: CreateOAuthInfoFromKeyValues, createOAuthConfig, newSocialBase
```
func NewAzureADProvider(settings map[string]any, cfg *setting.Cfg, features *featuremgmt.FeatureManager, cache remotecache.CacheStorage) (*SocialAzureAD, error) {
	info, err := CreateOAuthInfoFromKeyValues(settings)
	if err != nil {
		return nil, err
	}

	config := createOAuthConfig(info, cfg, AzureADProviderName)
	provider := &SocialAzureAD{
		SocialBase:           newSocialBase(AzureADProviderName, config, info, cfg.AutoAssignOrgRole, cfg.OAuthSkipOrgRoleUpdateSync, *features),
		cache:                cache,
		allowedOrganizations: util.SplitString(info.Extra[allowedOrganizationsKey]),
		forceUseGraphAPI:     MustBool(info.Extra[forceUseGraphAPIKey], false),
		skipOrgRoleSync:      cfg.AzureADSkipOrgRoleSync,
		// FIXME: Move skipOrgRoleSync to OAuthInfo
		// skipOrgRoleSync: info.SkipOrgRoleSync
	}

	if info.UseRefreshToken && features.IsEnabledGlobally(featuremgmt.FlagAccessTokenExpirationCheck) {
		appendUniqueScope(config, OfflineAccessScope)
	}

	return provider, nil
}
```

### `NewGenericOAuthProvider` (function, added) in generic_oauth.go:47-74
Called by: TestSearchJSONForEmail (generic_oauth_test.go), TestSearchJSONForGroups (generic_oauth_test.go), TestSearchJSONForRole (generic_oauth_test.go)
Calls: CreateOAuthInfoFromKeyValues, createOAuthConfig, newSocialBase
```
func NewGenericOAuthProvider(settings map[string]any, cfg *setting.Cfg, features *featuremgmt.FeatureManager) (*SocialGenericOAuth, error) {
	info, err := CreateOAuthInfoFromKeyValues(settings)
	if err != nil {
		return nil, err
	}

	config := createOAuthConfig(info, cfg, GenericOAuthProviderName)
	provider := &SocialGenericOAuth{
		SocialBase:           newSocialBase(GenericOAuthProviderName, config, info, cfg.AutoAssignOrgRole, cfg.OAuthSkipOrgRoleUpdateSync, *features),
		apiUrl:               info.ApiUrl,
		teamsUrl:             info.TeamsUrl,
		emailAttributeName:   info.EmailAttributeName,
		emailAttributePath:   info.EmailAttributePath,
		nameAttributePath:    info.Extra[nameAttributePathKey],
		groupsAttributePath:  info.GroupsAttributePath,
		loginAttributePath:   info.Extra[loginAttributePathKey],
		idTokenAttributeName: info.Extra[idTokenAttributeNameKey],
		teamIdsAttributePath: info.TeamIdsAttributePath,
		teamIds:              util.SplitString(info.Extra[teamIdsKey]),
		allowedOrganizations: util.SplitString(info.Extra[allowedOrganizationsKey]),
		allowedGroups:        info.AllowedGroups,
		skipOrgRoleSync:      cfg.GenericOAuthSkipOrgRoleSync,
		// FIXME: Move skipOr
... (truncated)
```

### `NewGitHubProvider` (function, added) in github_oauth.go:54-74
Called by: TestSocialGitHub_UserInfo (github_oauth_test.go), TestSocialGitHub_InitializeExtraFields (github_oauth_test.go), createOAuthConnector (social.go)
Calls: CreateOAuthInfoFromKeyValues, mustInts, createOAuthConfig
```
func NewGitHubProvider(settings map[string]any, cfg *setting.Cfg, features *featuremgmt.FeatureManager) (*SocialGithub, error) {
	info, err := CreateOAuthInfoFromKeyValues(settings)
	if err != nil {
		return nil, err
	}

	teamIds := mustInts(util.SplitString(info.Extra[teamIdsKey]))

	config := createOAuthConfig(info, cfg, GitHubProviderName)
	provider := &SocialGithub{
		SocialBase:           newSocialBase(GitHubProviderName, config, info, cfg.AutoAssignOrgRole, cfg.OAuthSkipOrgRoleUpdateSync, *features),
		apiUrl:               info.ApiUrl,
		teamIds:              teamIds,
		allowedOrganizations: util.SplitString(info.Extra[allowedOrganizationsKey]),
		skipOrgRoleSync:      cfg.GitHubSkipOrgRoleSync,
		// FIXME: Move skipOrgRoleSync to OAuthInfo
		// skipOrgRoleSync: info.SkipOrgRoleSync
	}

	return provider, nil
}
```

### `NewGitLabProvider` (function, added) in gitlab_oauth.go:52-68
Called by: TestSocialGitlab_UserInfo (gitlab_oauth_test.go), TestSocialGitlab_extractFromToken (gitlab_oauth_test.go), TestSocialGitlab_GetGroupsNextPage (gitlab_oauth_test.go)
Calls: CreateOAuthInfoFromKeyValues, createOAuthConfig, newSocialBase
```
func NewGitLabProvider(settings map[string]any, cfg *setting.Cfg, features *featuremgmt.FeatureManager) (*SocialGitlab, error) {
	info, err := CreateOAuthInfoFromKeyValues(settings)
	if err != nil {
		return nil, err
	}

	config := createOAuthConfig(info, cfg, GitlabProviderName)
	provider := &SocialGitlab{
		SocialBase:      newSocialBase(GitlabProviderName, config, info, cfg.AutoAssignOrgRole, cfg.OAuthSkipOrgRoleUpdateSync, *features),
		apiUrl:          info.ApiUrl,
		skipOrgRoleSync: cfg.GitLabSkipOrgRoleSync,
		// FIXME: Move skipOrgRoleSync to OAuthInfo
		// skipOrgRoleSync: info.SkipOrgRoleSync
	}

	return provider, nil
}
```

### `NewGoogleProvider` (function, added) in google_oauth.go:39-60
Called by: TestSocialGoogle_retrieveGroups (google_oauth_test.go), TestSocialGoogle_UserInfo (google_oauth_test.go), createOAuthConnector (social.go)
Calls: CreateOAuthInfoFromKeyValues, createOAuthConfig, newSocialBase
```
func NewGoogleProvider(settings map[string]any, cfg *setting.Cfg, features *featuremgmt.FeatureManager) (*SocialGoogle, error) {
	info, err := CreateOAuthInfoFromKeyValues(settings)
	if err != nil {
		return nil, err
	}

	config := createOAuthConfig(info, cfg, GoogleProviderName)
	provider := &SocialGoogle{
		SocialBase:      newSocialBase(GoogleProviderName, config, info, cfg.AutoAssignOrgRole, cfg.OAuthSkipOrgRoleUpdateSync, *features),
		hostedDomain:    info.HostedDomain,
		apiUrl:          info.ApiUrl,
		skipOrgRoleSync: cfg.GoogleSkipOrgRoleSync,
		// FIXME: Move skipOrgRoleSync to OAuthInfo
		// skipOrgRoleSync: info.SkipOrgRoleSync
	}

	if strings.HasPrefix(info.ApiUrl, legacyAPIURL) {
		provider.log.Warn("Using legacy Google API URL, please update your configuration")
	}

	return provider, nil
}
```

### `NewGrafanaComProvider` (function, added) in grafana_com_oauth.go:37-59
Called by: TestSocialGrafanaCom_UserInfo (grafana_com_oauth_test.go), TestSocialGrafanaCom_InitializeExtraFields (grafana_com_oauth_test.go), createOAuthConnector (social.go)
Calls: CreateOAuthInfoFromKeyValues, createOAuthConfig, newSocialBase
```
func NewGrafanaComProvider(settings map[string]any, cfg *setting.Cfg, features *featuremgmt.FeatureManager) (*SocialGrafanaCom, error) {
	info, err := CreateOAuthInfoFromKeyValues(settings)
	if err != nil {
		return nil, err
	}

	// Override necessary settings
	info.AuthUrl = cfg.GrafanaComURL + "/oauth2/authorize"
	info.TokenUrl = cfg.GrafanaComURL + "/api/oauth2/token"
	info.AuthStyle = "inheader"

	config := createOAuthConfig(info, cfg, GrafanaComProviderName)
	provider := &SocialGrafanaCom{
		SocialBase:           newSocialBase(GrafanaComProviderName, config, info, cfg.AutoAssignOrgRole, cfg.OAuthSkipOrgRoleUpdateSync, *features),
		url:                  cfg.GrafanaComURL,
		allowedOrganizations: util.SplitString(info.Extra[allowedOrganizationsKey]),
		skipOrgRoleSync:      cfg.GrafanaComSkipOrgRoleSync,
		// FIXME: Move skipOrgRoleSync to OAuthInfo
		// skipOrgRoleSync: info.SkipOrgRoleSync
	}

	return provider, nil
}
```

### `NewOktaProvider` (function, added) in okta_oauth.go:46-67
Called by: TestSocialOkta_UserInfo (okta_oauth_test.go), createOAuthConnector (social.go)
Calls: CreateOAuthInfoFromKeyValues, createOAuthConfig, newSocialBase
```
func NewOktaProvider(settings map[string]any, cfg *setting.Cfg, features *featuremgmt.FeatureManager) (*SocialOkta, error) {
	info, err := CreateOAuthInfoFromKeyValues(settings)
	if err != nil {
		return nil, err
	}

	config := createOAuthConfig(info, cfg, OktaProviderName)
	provider := &SocialOkta{
		SocialBase:    newSocialBase(OktaProviderName, config, info, cfg.AutoAssignOrgRole, cfg.OAuthSkipOrgRoleUpdateSync, *features),
		apiUrl:        info.ApiUrl,
		allowedGroups: info.AllowedGroups,
		// FIXME: Move skipOrgRoleSync to OAuthInfo
		// skipOrgRoleSync: info.SkipOrgRoleSync
		skipOrgRoleSync: cfg.OktaSkipOrgRoleSync,
	}

	if info.UseRefreshToken && features.IsEnabledGlobally(featuremgmt.FlagAccessTokenExpirationCheck) {
		appendUniqueScope(config, OfflineAccessScope)
	}

	return provider, nil
}
```

### `ProvideService` (function, added) in social.go:91-134
Calls: getUsageStats, convertIniSectionToMap, CreateOAuthInfoFromKeyValues
```
func ProvideService(cfg *setting.Cfg,
	features *featuremgmt.FeatureManager,
	usageStats usagestats.Service,
	bundleRegistry supportbundles.Service,
	cache remotecache.CacheStorage,
) *SocialService {
	ss := &SocialService{
		cfg:       cfg,
		socialMap: make(map[string]SocialConnector),
		log:       log.New("login.social"),
	}

	usageStats.RegisterMetricsFunc(ss.getUsageStats)

	for _, name := range allOauthes {
		sec := cfg.Raw.Section("auth." + name)

		settingsKVs := convertIniSectionToMap(sec)
		info, err := CreateOAuthInfoFromKeyValues(settingsKVs)
		if err != nil {
			ss.log.Error("Failed to create OAuthInfo for provider", "error", err, "provider", name)
			continue
		}

		if !info.Enabled {
			continue
		}

		if name == GrafanaNetProviderName {
			name = GrafanaComProviderName
		}

		conn, err := ss.createOAuthConnector(name, settingsKVs, cfg, features, cache)
		if err != nil {
			ss.log.Error("Failed to create OAuth provider", "error", err, "provider", name)
		}

		ss.socialMap[name] = conn
	}

	ss.registerSupportBundleCollectors(bundleRegistry)

	return ss
}
```

### `createOAuthConfig` (function, added) in common.go:150-174
Called by: NewAzureADProvider (azuread_oauth.go), NewGenericOAuthProvider (generic_oauth.go), NewGitHubProvider (github_oauth.go)
```
func createOAuthConfig(info *OAuthInfo, cfg *setting.Cfg, defaultName string) *oauth2.Config {
	var authStyle oauth2.AuthStyle
	switch strings.ToLower(info.AuthStyle) {
	case "inparams":
		authStyle = oauth2.AuthStyleInParams
	case "inheader":
		authStyle = oauth2.AuthStyleInHeader
	default:
		authStyle = oauth2.AuthStyleAutoDetect
	}

	config := oauth2.Config{
		ClientID:     info.ClientId,
		ClientSecret: info.ClientSecret,
		Endpoint: oauth2.Endpoint{
			AuthURL:   info.AuthUrl,
			TokenURL:  info.TokenUrl,
			AuthStyle: authStyle,
		},
		RedirectURL: strings.TrimSuffix(cfg.AppURL, "/") + SocialBaseUrl + defaultName,
		Scopes:      info.Scopes,
	}

	return &config
}
```

### `newSocialBase` (function, added) in social.go:206-230
Called by: NewAzureADProvider (azuread_oauth.go), NewGenericOAuthProvider (generic_oauth.go), NewGitHubProvider (github_oauth.go)
```
func newSocialBase(name string,
	config *oauth2.Config,
	info *OAuthInfo,
	autoAssignOrgRole string,
	skipOrgRoleSync bool,
	features featuremgmt.FeatureManager,
) *SocialBase {
	logger := log.New("oauth." + name)

	return &SocialBase{
		Config:                  config,
		info:                    info,
		log:                     logger,
		allowSignup:             info.AllowSignup,
		allowAssignGrafanaAdmin: info.AllowAssignGrafanaAdmin,
		allowedDomains:          info.AllowedDomains,
		allowedGroups:           info.AllowedGroups,
		roleAttributePath:       info.RoleAttributePath,
		roleAttributeStrict:     info.RoleAttributeStrict,
		autoAssignOrgRole:       autoAssignOrgRole,
		skipOrgRoleSync:         skipOrgRoleSync,
		features:                features,
		useRefreshToken:         info.UseRefreshToken,
	}
}
```

### `appendUniqueScope` (function, added) in social.go:536-540
Called by: NewAzureADProvider (azuread_oauth.go), NewOktaProvider (okta_oauth.go)
```
func appendUniqueScope(config *oauth2.Config, scope string) {
	if !slices.Contains(config.Scopes, OfflineAccessScope) {
		config.Scopes = append(config.Scopes, OfflineAccessScope)
	}
}
```

### `createOAuthConnector` (method, added) in social.go:515-534
Called by: ProvideService (social.go)
Calls: NewAzureADProvider, NewGenericOAuthProvider, NewGitHubProvider
```
func (ss *SocialService) createOAuthConnector(name string, settings map[string]any, cfg *setting.Cfg, features *featuremgmt.FeatureManager, cache remotecache.CacheStorage) (SocialConnector, error) {
	switch name {
	case AzureADProviderName:
		return NewAzureADProvider(settings, cfg, features, cache)
	case GenericOAuthProviderName:
		return NewGenericOAuthProvider(settings, cfg, features)
	case GitHubProviderName:
		return NewGitHubProvider(settings, cfg, features)
	case GitlabProviderName:
		return NewGitLabProvider(settings, cfg, features)
	case GoogleProviderName:
		return NewGoogleProvider(settings, cfg, features)
	case GrafanaComProviderName:
		return NewGrafanaComProvider(settings, cfg, features)
	case OktaProviderName:
		return NewOktaProvider(settings, cfg, features)
	default:
		return nil, fmt.Errorf("unknown oauth provider: %s", name)
	}
}
```

## Hypotheses to verify


---

# GrafanaConfig (interface)

# Slice 7: GrafanaConfig (interface, modified)
## Anchor: `GrafanaConfig` in packages/grafana-data/src/types/config.ts:149-229
risk=Critical(0.77) blast=10000 deps=13

## Detector Findings
- **[MEDIUM] variable-near-miss**: `GrafanaConfig` — Identifier changed from `googleAnalytics4Id` to similar `googleAnalyticsId` — possible wrong-variable usage
  Evidence: `googleAnalyticsId: string | undefined;`
- **[MEDIUM] variable-near-miss**: `GrafanaBootConfig` — Identifier changed from `googleAnalytics4Id` to similar `googleAnalyticsId` — possible wrong-variable usage
  Evidence: `googleAnalyticsId: undefined;`
- **[MEDIUM] type-change-propagation**: `GrafanaBootConfig` — Type `GrafanaBootConfig` was modified but 15 dependent(s) were not updated in this diff: constructor, overrideFeatureTogglesFromLocalStorage, overrideFeatureTogglesFromUrl, config, grafanaConfig and 10 more
  Evidence: `Unchanged dependents: constructor, overrideFeatureTogglesFromLocalStorage, overrideFeatureTogglesFromUrl, config, grafanaConfig and 10 more`
- **[MEDIUM] type-change-propagation**: `GrafanaConfig` — Type `GrafanaConfig` was modified but 12 dependent(s) were not updated in this diff: BootData, grafanaConfig, LocationUtilDependencies, CheckHealth, QueryData and 7 more
  Evidence: `Unchanged dependents: BootData, grafanaConfig, LocationUtilDependencies, CheckHealth, QueryData and 7 more`

## Code
### `GrafanaConfig` (interface, modified) in config.ts:149-229
Called by: BootData (config.ts), grafanaConfig (location.ts), LocationUtilDependencies (location.ts)
Calls: anonymousDeviceLimit
```
interface GrafanaConfig {
  publicDashboardAccessToken?: string;
  snapshotEnabled: boolean;
  datasources: { [str: string]: DataSourceInstanceSettings };
  panels: { [key: string]: PanelPluginMeta };
  auth: AuthSettings;
  minRefreshInterval: string;
  appSubUrl: string;
  windowTitlePrefix: string;
  buildInfo: BuildInfo;
  newPanelTitle: string;
  bootData: BootData;
  externalUserMngLinkUrl: string;
  externalUserMngLinkName: string;
  externalUserMngInfo: string;
  allowOrgCreate: boolean;
  disableLoginForm: boolean;
  defaultDatasource: string;
  alertingEnabled: boolean;
  alertingErrorOrTimeout: string;
  alertingNoDataOrNullValues: string;
  alertingMinInterval: number;
  authProxyEnabled: boolean;
  exploreEnabled: boolean;
  queryHistoryEnabled: boolean;
  helpEnabled: boolean;
  profileEnabled: boolean;
  newsFeedEnabled: boolean;
  ldapEnabled: boolean;
  sigV4AuthEnabled: boolean;
  azureAuthEnabled: boolean;
  samlEnabled: boolean;
  autoAssignOrg: boolean;
  verifyEmailEnabled: boolean;
  oauth: OAuthSettings;
  /** @deprecated always set to true. */
  rbacEnabled: boolean;
  disableUserSignUp: boolean;
  loginHint: string;
  passwordHint: string;
  loginError?: s
... (truncated)
```

### `anonymousDeviceLimit` (field, added) in config.ts:97-97
Called by: GrafanaConfig (config.ts), GrafanaBootConfig (config.ts)
```
anonymousDeviceLimit = undefined
```

### `GrafanaBootConfig` (class, modified) in config.ts:37-214
Called by: constructor (config.ts), overrideFeatureTogglesFromLocalStorage (config.ts), overrideFeatureTogglesFromUrl (config.ts)
Calls: GrafanaConfig, anonymousDeviceLimit
```
class GrafanaBootConfig implements GrafanaConfig {
  publicDashboardAccessToken?: string;
  snapshotEnabled = true;
  datasources: { [str: string]: DataSourceInstanceSettings } = {};
  panels: { [key: string]: PanelPluginMeta } = {};
  apps: Record<string, AppPluginConfig> = {};
  auth: AuthSettings = {};
  minRefreshInterval = '';
  appUrl = '';
  appSubUrl = '';
  namespace = 'default';
  windowTitlePrefix = '';
  buildInfo: BuildInfo;
  newPanelTitle = '';
  bootData: BootData;
  externalUserMngLinkUrl = '';
  externalUserMngLinkName = '';
  externalUserMngInfo = '';
  allowOrgCreate = false;
  feedbackLinksEnabled = true;
  disableLoginForm = false;
  defaultDatasource = ''; // UID
  alertingEnabled = false;
  alertingErrorOrTimeout = '';
  alertingNoDataOrNullValues = '';
  alertingMinInterval = 1;
  angularSupportEnabled = false;
  authProxyEnabled = false;
  exploreEnabled = false;
  queryHistoryEnabled = false;
  helpEnabled = false;
  profileEnabled = false;
  newsFeedEnabled = true;
  ldapEnabled = false;
  jwtHeaderName = '';
  jwtUrlLogin = false;
  sigV4AuthEnabled = false;
  azureAuthEnabled = false;
  secureSocksDSProxyEnabled = false;
  samlEnabled = false;
  samlNa
... (truncated)
```

## Hypotheses to verify
- Type `GrafanaBootConfig` changed but dependents not updated. Check usages.
- Type `GrafanaConfig` changed but dependents not updated. Check usages.


---

# retrieveJWKSFromCache (method)

# Slice 8: retrieveJWKSFromCache (method, added)
## Anchor: `retrieveJWKSFromCache` in pkg/login/social/azuread_jwks.go:21-36
risk=High(0.63) blast=17 deps=1

## Detector Findings
- **[HIGH] nil-check-missing**: `retrieveJWKSFromCache` — Error return value not checked — may use nil/zero value from failed call
  Evidence: `if val, err := s.cache.Get(ctx, cacheKey); err == nil {`

## Code
### `retrieveJWKSFromCache` (method, added) in azuread_jwks.go:21-36
Called by: validateIDTokenSignature (azuread_oauth.go)
Calls: Client, getJWKSCacheKey
```
func (s *SocialAzureAD) retrieveJWKSFromCache(ctx context.Context, client *http.Client, authURL string) (*keySetJWKS, time.Duration, error) {
	cacheKey, err := s.getJWKSCacheKey()
	if err != nil {
		return nil, 0, err
	}

	if val, err := s.cache.Get(ctx, cacheKey); err == nil {
		var jwks keySetJWKS
		err := json.Unmarshal(val, &jwks)
		s.log.Debug("Retrieved cached key set", "cacheKey", cacheKey)
		return &jwks, 0, err
	}
	s.log.Debug("Keyset not found in cache", "err", err)

	return &keySetJWKS{}, 0, nil
}
```

### `getJWKSCacheKey` (method, added) in azuread_jwks.go:18-20
Called by: retrieveJWKSFromCache (azuread_jwks.go), cacheJWKS (azuread_jwks.go)
```
func (s *SocialAzureAD) getJWKSCacheKey() (string, error) {
	return azureCacheKeyPrefix + s.ClientID, nil
}
```

### `validateIDTokenSignature` (method, added) in azuread_oauth.go:195-230
Called by: validateClaims (azuread_oauth.go)
Calls: Client, retrieveJWKSFromCache, retrieveSpecificJWKS
```
func (s *SocialAzureAD) validateIDTokenSignature(ctx context.Context, client *http.Client, parsedToken *jwt.JSONWebToken) (*azureClaims, error) {
	var claims azureClaims

	jwksFuncs := []func(ctx context.Context, client *http.Client, authURL string) (*keySetJWKS, time.Duration, error){
		s.retrieveJWKSFromCache, s.retrieveSpecificJWKS, s.retrieveGeneralJWKS,
	}

	keyID := parsedToken.Headers[0].KeyID

	for _, jwksFunc := range jwksFuncs {
		keyset, expiry, err := jwksFunc(ctx, client, s.Endpoint.AuthURL)
		if err != nil {
			return nil, fmt.Errorf("error retrieving jwks: %w", err)
		}
		var errClaims error
		keys := keyset.Key(keyID)
		for _, key := range keys {
			s.log.Debug("AzureAD OAuth: trying to parse token with key", "kid", key.KeyID)
			if errClaims = parsedToken.Claims(key, &claims); errClaims == nil {
				if expiry != 0 {
					s.log.Debug("AzureAD OAuth: caching key set", "kid", key.KeyID, "expiry", expiry)
					if err := s.cacheJWKS(ctx, keyset, expiry); err != nil {
						s.log.Warn("Failed to set key set in cache", "err", err)
					}
				}
				return &claims, nil
			} else {
				s.log.Warn("AzureAD OAuth: failed to parse token with key", "kid", key.KeyID, "err", errCl
... (truncated)
```

### `cacheJWKS` (method, added) in azuread_jwks.go:38-54
Called by: validateIDTokenSignature (azuread_oauth.go)
Calls: getJWKSCacheKey
```
func (s *SocialAzureAD) cacheJWKS(ctx context.Context, jwks *keySetJWKS, cacheExpiration time.Duration) error {
	cacheKey, err := s.getJWKSCacheKey()
	if err != nil {
		return err
	}

	var jsonBuf bytes.Buffer
	if err := json.NewEncoder(&jsonBuf).Encode(jwks); err != nil {
		return err
	}

	if err := s.cache.Set(ctx, cacheKey, jsonBuf.Bytes(), cacheExpiration); err != nil {
		s.log.Warn("Failed to cache key set", "err", err)
	}

	return nil
}
```

### `retrieveSpecificJWKS` (method, added) in azuread_jwks.go:76-94
Called by: validateIDTokenSignature (azuread_oauth.go)
Calls: Client, httpGet, getCacheExpiration
```
func (s *SocialAzureAD) retrieveSpecificJWKS(ctx context.Context, client *http.Client, authURL string) (*keySetJWKS, time.Duration, error) {
	keysetURL := strings.Replace(authURL, "/oauth2/v2.0/authorize", "/discovery/v2.0/keys", 1) + "?appid=" + s.ClientID

	resp, err := s.httpGet(ctx, client, keysetURL)
	if err != nil {
		return nil, 0, err
	}

	bytesReader := bytes.NewReader(resp.Body)
	var jwks keySetJWKS
	if err := json.NewDecoder(bytesReader).Decode(&jwks); err != nil {
		return nil, 0, err
	}

	cacheExpiration := getCacheExpiration(resp.Headers.Get("cache-control"))
	s.log.Debug("Retrieved specific key set", "url", keysetURL, "cacheExpiration", cacheExpiration)

	return &jwks, cacheExpiration, nil
}
```

### `retrieveGeneralJWKS` (method, added) in azuread_jwks.go:56-74
Called by: validateIDTokenSignature (azuread_oauth.go)
Calls: Client, httpGet, getCacheExpiration
```
func (s *SocialAzureAD) retrieveGeneralJWKS(ctx context.Context, client *http.Client, authURL string) (*keySetJWKS, time.Duration, error) {
	keysetURL := strings.Replace(authURL, "/oauth2/v2.0/authorize", "/discovery/v2.0/keys", 1)

	resp, err := s.httpGet(ctx, client, keysetURL)
	if err != nil {
		return nil, 0, err
	}

	bytesReader := bytes.NewReader(resp.Body)
	var jwks keySetJWKS
	if err := json.NewDecoder(bytesReader).Decode(&jwks); err != nil {
		return nil, 0, err
	}

	cacheExpiration := getCacheExpiration(resp.Headers.Get("cache-control"))
	s.log.Debug("Retrieved general key set", "url", keysetURL, "cacheExpiration", cacheExpiration)

	return &jwks, cacheExpiration, nil
}
```

### `validateClaims` (method, added) in azuread_oauth.go:173-193
Called by: UserInfo (azuread_oauth.go), Verify (auth.go)
Calls: Client, validateIDTokenSignature, isAllowedTenant
```
func (s *SocialAzureAD) validateClaims(ctx context.Context, client *http.Client, parsedToken *jwt.JSONWebToken) (*azureClaims, error) {
	claims, err := s.validateIDTokenSignature(ctx, client, parsedToken)
	if err != nil {
		return nil, fmt.Errorf("error getting claims from id token: %w", err)
	}

	if claims.OAuthVersion == "1.0" {
		return nil, &Error{"AzureAD OAuth: version 1.0 is not supported. Please ensure the auth_url and token_url are set to the v2.0 endpoints."}
	}

	s.log.Debug("Validating audience", "audience", claims.Audience, "client_id", s.ClientID)
	if claims.Audience != s.ClientID {
		return nil, &Error{"AzureAD OAuth: audience mismatch"}
	}

	s.log.Debug("Validating tenant", "tenant", claims.TenantID, "allowed_tenants", s.allowedOrganizations)
	if !s.isAllowedTenant(claims.TenantID) {
		return nil, &Error{"AzureAD OAuth: tenant mismatch"}
	}
	return claims, nil
}
```

### `httpGet` (method, added) in common.go:60-91
Called by: retrieveGeneralJWKS (azuread_jwks.go), retrieveSpecificJWKS (azuread_jwks.go), extractFromAPI (generic_oauth.go)
Calls: Client
```
func (s *SocialBase) httpGet(ctx context.Context, client *http.Client, url string) (*httpGetResponse, error) {
	req, errReq := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if errReq != nil {
		return nil, errReq
	}

	r, errDo := client.Do(req)
	if errDo != nil {
		return nil, errDo
	}

	defer func() {
		if err := r.Body.Close(); err != nil {
			s.log.Warn("Failed to close response body", "err", err)
		}
	}()

	body, errRead := io.ReadAll(r.Body)
	if errRead != nil {
		return nil, errRead
	}

	response := &httpGetResponse{body, r.Header}

	if r.StatusCode >= 300 {
		return nil, fmt.Errorf("unsuccessful response status code %d: %s", r.StatusCode, string(response.Body))
	}

	s.log.Debug("HTTP GET", "url", url, "status", r.Status, "response_body", string(response.Body))

	return response, nil
}
```

### `getCacheExpiration` (function, added) in azuread_jwks.go:96-117
Called by: retrieveGeneralJWKS (azuread_jwks.go), retrieveSpecificJWKS (azuread_jwks.go), TestGetCacheExpiration (key_sets_test.go)
```
func getCacheExpiration(header string) time.Duration {
	if header == "" {
		return defaultCacheExpiration
	}

	// Cache-Control: public, max-age=14400
	cacheControl := strings.Split(header, ",")
	for _, v := range cacheControl {
		if strings.Contains(v, "max-age") {
			parts := strings.Split(v, "=")
			if len(parts) == 2 {
				seconds, err := strconv.Atoi(parts[1])
				if err != nil {
					return defaultCacheExpiration
				}
				return time.Duration(seconds) * time.Second
			}
		}
	}

	return defaultCacheExpiration
}
```

### `isAllowedTenant` (method, added) in azuread_oauth.go:396-408
Called by: validateClaims (azuread_oauth.go)
```
func (s *SocialAzureAD) isAllowedTenant(tenantID string) bool {
	if len(s.allowedOrganizations) == 0 {
		s.log.Warn("No allowed organizations specified, all tenants are allowed. Configure allowed_organizations to restrict access")
		return true
	}

	for _, t := range s.allowedOrganizations {
		if t == tenantID {
			return true
		}
	}
	return false
}
```

### `UserInfo` (method, added) in azuread_oauth.go:99-167
Called by: TestSocialAzureAD_UserInfo (azuread_oauth_test.go), TestSocialAzureAD_SkipOrgRole (azuread_oauth_test.go), TestUserInfoSearchesForEmailAndRole (generic_oauth_test.go)
Calls: Client, validateClaims, extractEmail
```
func (s *SocialAzureAD) UserInfo(ctx context.Context, client *http.Client, token *oauth2.Token) (*BasicUserInfo, error) {
	idToken := token.Extra("id_token")
	if idToken == nil {
		return nil, ErrIDTokenNotFound
	}

	parsedToken, err := jwt.ParseSigned(idToken.(string))
	if err != nil {
		return nil, fmt.Errorf("error parsing id token: %w", err)
	}

	claims, err := s.validateClaims(ctx, client, parsedToken)
	if err != nil {
		return nil, err
	}

	email := claims.extractEmail()
	if email == "" {
		return nil, ErrEmailNotFound
	}

	// setting the role, grafanaAdmin to empty to reflect that we are not syncronizing with the external provider
	var role roletype.RoleType
	var grafanaAdmin bool
	if !s.skipOrgRoleSync {
		role, grafanaAdmin, err = s.extractRoleAndAdmin(claims)
		if err != nil {
			return nil, err
		}

		if !role.IsValid() {
			return nil, errInvalidRole.Errorf("AzureAD OAuth: invalid role %q", role)
		}
	}
	s.log.Debug("AzureAD OAuth: extracted role", "email", email, "role", role)

	groups, err := s.extractGroups(ctx, client, claims, token)
	if err != nil {
		return nil, fmt.Errorf("failed to extract groups: %w", err)
	}
	s.log.Debug("AzureAD OAuth: extracted groups", "ema
... (truncated)
```

## Hypotheses to verify


---

# supportBundleCollectorFn (method)

# Slice 9: supportBundleCollectorFn (method, added)
## Anchor: `supportBundleCollectorFn` in pkg/login/social/support_bundle.go:29-62
risk=High(0.62) blast=2 deps=1

## Detector Findings
- **[HIGH] nil-check-missing**: `supportBundleCollectorFn` — Error return value not checked — may use nil/zero value from failed call
  Evidence: `if _, err := bWriter.WriteString(fmt.Sprintf("# OAuth %s information\n\n", name)); err != nil {`

## Code
### `supportBundleCollectorFn` (method, added) in support_bundle.go:29-62
Called by: registerSupportBundleCollectors (support_bundle.go)
Calls: GetOAuthInfo, SupportBundleContent, healthCheckSocialConnector
```
func (ss *SocialService) supportBundleCollectorFn(name string, sc SocialConnector) func(context.Context) (*supportbundles.SupportItem, error) {
	return func(ctx context.Context) (*supportbundles.SupportItem, error) {
		bWriter := bytes.NewBuffer(nil)

		if _, err := bWriter.WriteString(fmt.Sprintf("# OAuth %s information\n\n", name)); err != nil {
			return nil, err
		}

		if _, err := bWriter.WriteString("## Parsed Configuration\n\n"); err != nil {
			return nil, err
		}

		oinfo := sc.GetOAuthInfo()

		bWriter.WriteString("```toml\n")
		errM := toml.NewEncoder(bWriter).Encode(oinfo)
		if errM != nil {
			bWriter.WriteString(
				fmt.Sprintf("Unable to encode OAuth configuration  \n Err: %s", errM))
		}
		bWriter.WriteString("```\n\n")

		if err := sc.SupportBundleContent(bWriter); err != nil {
			return nil, err
		}

		ss.healthCheckSocialConnector(ctx, name, oinfo, bWriter)

		return &supportbundles.SupportItem{
			Filename:  "oauth-" + name + ".md",
			FileBytes: bWriter.Bytes(),
		}, nil
	}
}
```

### `GetOAuthInfo` (method, added) in azuread_oauth.go:169-171
Called by: GetOAuthProviders (social.go), GetOAuthHttpClient (social.go), GetOAuthInfoProvider (social.go)
```
func (s *SocialAzureAD) GetOAuthInfo() *OAuthInfo {
	return s.info
}
```

### `SupportBundleContent` (method, added) in azuread_oauth.go:386-394
Called by: supportBundleCollectorFn (support_bundle.go)
```
func (s *SocialAzureAD) SupportBundleContent(bf *bytes.Buffer) error {
	bf.WriteString("## AzureAD specific configuration\n\n")
	bf.WriteString("```ini\n")
	bf.WriteString(fmt.Sprintf("allowed_groups = %v\n", s.allowedGroups))
	bf.WriteString(fmt.Sprintf("forceUseGraphAPI = %v\n", s.forceUseGraphAPI))
	bf.WriteString("```\n\n")

	return s.SocialBase.SupportBundleContent(bf)
}
```

### `healthCheckSocialConnector` (method, added) in support_bundle.go:64-76
Called by: supportBundleCollectorFn (support_bundle.go)
Calls: healthCheckEndpoint
```
func (ss *SocialService) healthCheckSocialConnector(ctx context.Context, name string, oinfo *OAuthInfo, bWriter *bytes.Buffer) {
	bWriter.WriteString("## Health checks\n\n")
	client, err := ss.GetOAuthHttpClient(name)
	if err != nil {
		bWriter.WriteString(fmt.Sprintf("Unable to create HTTP client  \n Err: %s\n", err))
		return
	}

	healthCheckEndpoint(client, bWriter, "API", oinfo.ApiUrl)
	healthCheckEndpoint(client, bWriter, "Auth", oinfo.AuthUrl)
	healthCheckEndpoint(client, bWriter, "Token", oinfo.TokenUrl)
	healthCheckEndpoint(client, bWriter, "Teams", oinfo.TeamsUrl)
}
```

### `registerSupportBundleCollectors` (method, added) in support_bundle.go:15-27
Called by: ProvideService (social.go)
Calls: GetOAuthInfo, supportBundleCollectorFn
```
func (ss *SocialService) registerSupportBundleCollectors(bundleRegistry supportbundles.Service) {
	for name, connector := range ss.socialMap {
		bundleRegistry.RegisterSupportItemCollector(supportbundles.Collector{
			UID:               "oauth-" + name,
			DisplayName:       "OAuth " + strings.Title(strings.ReplaceAll(name, "_", " ")),
			Description:       "OAuth configuration and healthchecks for " + name,
			IncludedByDefault: false,
			Default:           false,
			EnabledFn:         func() bool { return connector.GetOAuthInfo().Enabled },
			Fn:                ss.supportBundleCollectorFn(name, connector),
		})
	}
}
```

### `GetOAuthProviders` (method, added) in social.go:326-334
Called by: getUsageStats (social.go)
Calls: GetOAuthInfo
```
func (ss *SocialService) GetOAuthProviders() map[string]bool {
	result := map[string]bool{}

	for name, conn := range ss.socialMap {
		result[name] = conn.GetOAuthInfo().Enabled
	}

	return result
}
```

### `GetOAuthHttpClient` (method, added) in social.go:336-391
Calls: Client, GetOAuthInfo, Error
```
func (ss *SocialService) GetOAuthHttpClient(name string) (*http.Client, error) {
	// The socialMap keys don't have "oauth_" prefix, but everywhere else in the system does
	name = strings.TrimPrefix(name, "oauth_")
	provider, ok := ss.socialMap[name]
	if !ok {
		return nil, fmt.Errorf("could not find %q in OAuth Settings", name)
	}

	info := provider.GetOAuthInfo()
	if !info.Enabled {
		return nil, fmt.Errorf("oauth provider %q is not enabled", name)
	}

	// handle call back
	tr := &http.Transport{
		Proxy: http.ProxyFromEnvironment,
		TLSClientConfig: &tls.Config{
			InsecureSkipVerify: info.TlsSkipVerify,
		},
		DialContext: (&net.Dialer{
			Timeout:   time.Second * 10,
			KeepAlive: 30 * time.Second,
		}).DialContext,
		TLSHandshakeTimeout:   15 * time.Second,
		ExpectContinueTimeout: 1 * time.Second,
		MaxIdleConns:          100,
		IdleConnTimeout:       90 * time.Second,
	}

	oauthClient := &http.Client{
		Transport: tr,
		Timeout:   time.Second * 15,
	}

	if info.TlsClientCert != "" || info.TlsClientKey != "" {
		cert, err := tls.LoadX509KeyPair(info.TlsClientCert, info.TlsClientKey)
		if err != nil {
			ss.log.Error("Failed to setup TlsClientCert", "oauth", name, "error", err
... (truncated)
```

### `GetOAuthInfoProvider` (method, added) in social.go:403-409
Calls: GetOAuthInfo
```
func (ss *SocialService) GetOAuthInfoProvider(name string) *OAuthInfo {
	connector, ok := ss.socialMap[name]
	if !ok {
		return nil
	}
	return connector.GetOAuthInfo()
}
```

### `GetOAuthInfoProviders` (method, added) in social.go:412-421
Calls: GetOAuthInfo
```
func (ss *SocialService) GetOAuthInfoProviders() map[string]*OAuthInfo {
	result := map[string]*OAuthInfo{}
	for name, connector := range ss.socialMap {
		info := connector.GetOAuthInfo()
		if info.Enabled {
			result[name] = info
		}
	}
	return result
}
```

### `healthCheckEndpoint` (function, added) in support_bundle.go:78-91
Called by: healthCheckSocialConnector (support_bundle.go)
Calls: Client
```
func healthCheckEndpoint(client *http.Client, bWriter *bytes.Buffer, endpointName string, url string) {
	if url == "" {
		return
	}

	bWriter.WriteString(fmt.Sprintf("### %s URL\n\n", endpointName))
	resp, err := client.Get(url)
	_ = resp.Body.Close()
	if err != nil {
		bWriter.WriteString(fmt.Sprintf("Unable to GET %s URL  \n Err: %s\n\n", endpointName, err))
	} else {
		bWriter.WriteString(fmt.Sprintf("Able to reach %s URL. Status Code does not need to be 200.\n Retrieved Status Code: %d \n\n", endpointName, resp.StatusCode))
	}
}
```

### `ProvideService` (function, added) in social.go:91-134
Calls: getUsageStats, convertIniSectionToMap, CreateOAuthInfoFromKeyValues
```
func ProvideService(cfg *setting.Cfg,
	features *featuremgmt.FeatureManager,
	usageStats usagestats.Service,
	bundleRegistry supportbundles.Service,
	cache remotecache.CacheStorage,
) *SocialService {
	ss := &SocialService{
		cfg:       cfg,
		socialMap: make(map[string]SocialConnector),
		log:       log.New("login.social"),
	}

	usageStats.RegisterMetricsFunc(ss.getUsageStats)

	for _, name := range allOauthes {
		sec := cfg.Raw.Section("auth." + name)

		settingsKVs := convertIniSectionToMap(sec)
		info, err := CreateOAuthInfoFromKeyValues(settingsKVs)
		if err != nil {
			ss.log.Error("Failed to create OAuthInfo for provider", "error", err, "provider", name)
			continue
		}

		if !info.Enabled {
			continue
		}

		if name == GrafanaNetProviderName {
			name = GrafanaComProviderName
		}

		conn, err := ss.createOAuthConnector(name, settingsKVs, cfg, features, cache)
		if err != nil {
			ss.log.Error("Failed to create OAuth provider", "error", err, "provider", name)
		}

		ss.socialMap[name] = conn
	}

	ss.registerSupportBundleCollectors(bundleRegistry)

	return ss
}
```

### `getUsageStats` (method, added) in social.go:423-441
Called by: ProvideService (social.go)
Calls: GetOAuthProviders
```
func (ss *SocialService) getUsageStats(ctx context.Context) (map[string]any, error) {
	m := map[string]any{}

	authTypes := map[string]bool{}
	for provider, enabled := range ss.GetOAuthProviders() {
		authTypes["oauth_"+provider] = enabled
	}

	for authType, enabled := range authTypes {
		enabledValue := 0
		if enabled {
			enabledValue = 1
		}

		m["stats.auth_enabled."+authType+".count"] = enabledValue
	}

	return m, nil
}
```

### `Error` (method, added) in social.go:187-189
Called by: ProvideService (social.go), GetOAuthHttpClient (social.go)
```
func (e Error) Error() string {
	return e.s
}
```

## Hypotheses to verify


---

# UserInfo (method)

# Slice 10: UserInfo (method, added)
## Anchor: `UserInfo` in pkg/login/social/azuread_oauth.go:99-167
risk=Critical(0.88) blast=13 deps=13

## Code
### `UserInfo` (method, added) in azuread_oauth.go:99-167
Called by: TestSocialAzureAD_UserInfo (azuread_oauth_test.go), TestSocialAzureAD_SkipOrgRole (azuread_oauth_test.go), TestUserInfoSearchesForEmailAndRole (generic_oauth_test.go)
Calls: Client, validateClaims, extractEmail
```
func (s *SocialAzureAD) UserInfo(ctx context.Context, client *http.Client, token *oauth2.Token) (*BasicUserInfo, error) {
	idToken := token.Extra("id_token")
	if idToken == nil {
		return nil, ErrIDTokenNotFound
	}

	parsedToken, err := jwt.ParseSigned(idToken.(string))
	if err != nil {
		return nil, fmt.Errorf("error parsing id token: %w", err)
	}

	claims, err := s.validateClaims(ctx, client, parsedToken)
	if err != nil {
		return nil, err
	}

	email := claims.extractEmail()
	if email == "" {
		return nil, ErrEmailNotFound
	}

	// setting the role, grafanaAdmin to empty to reflect that we are not syncronizing with the external provider
	var role roletype.RoleType
	var grafanaAdmin bool
	if !s.skipOrgRoleSync {
		role, grafanaAdmin, err = s.extractRoleAndAdmin(claims)
		if err != nil {
			return nil, err
		}

		if !role.IsValid() {
			return nil, errInvalidRole.Errorf("AzureAD OAuth: invalid role %q", role)
		}
	}
	s.log.Debug("AzureAD OAuth: extracted role", "email", email, "role", role)

	groups, err := s.extractGroups(ctx, client, claims, token)
	if err != nil {
		return nil, fmt.Errorf("failed to extract groups: %w", err)
	}
	s.log.Debug("AzureAD OAuth: extracted groups", "ema
... (truncated)
```

### `validateClaims` (method, added) in azuread_oauth.go:173-193
Called by: UserInfo (azuread_oauth.go), Verify (auth.go)
Calls: Client, validateIDTokenSignature, isAllowedTenant
```
func (s *SocialAzureAD) validateClaims(ctx context.Context, client *http.Client, parsedToken *jwt.JSONWebToken) (*azureClaims, error) {
	claims, err := s.validateIDTokenSignature(ctx, client, parsedToken)
	if err != nil {
		return nil, fmt.Errorf("error getting claims from id token: %w", err)
	}

	if claims.OAuthVersion == "1.0" {
		return nil, &Error{"AzureAD OAuth: version 1.0 is not supported. Please ensure the auth_url and token_url are set to the v2.0 endpoints."}
	}

	s.log.Debug("Validating audience", "audience", claims.Audience, "client_id", s.ClientID)
	if claims.Audience != s.ClientID {
		return nil, &Error{"AzureAD OAuth: audience mismatch"}
	}

	s.log.Debug("Validating tenant", "tenant", claims.TenantID, "allowed_tenants", s.allowedOrganizations)
	if !s.isAllowedTenant(claims.TenantID) {
		return nil, &Error{"AzureAD OAuth: tenant mismatch"}
	}
	return claims, nil
}
```

### `extractEmail` (method, added) in azuread_oauth.go:232-240
Called by: UserInfo (azuread_oauth.go)
```
func (claims *azureClaims) extractEmail() string {
	if claims.Email == "" {
		if claims.PreferredUsername != "" {
			return claims.PreferredUsername
		}
	}

	return claims.Email
}
```

### `extractRoleAndAdmin` (method, added) in azuread_oauth.go:243-268
Called by: UserInfo (azuread_oauth.go), UserInfo (github_oauth.go), extractFromAPI (gitlab_oauth.go)
Calls: defaultRole, hasRole
```
func (s *SocialAzureAD) extractRoleAndAdmin(claims *azureClaims) (org.RoleType, bool, error) {
	if len(claims.Roles) == 0 {
		if s.roleAttributeStrict {
			return "", false, errRoleAttributeStrictViolation.Errorf("AzureAD OAuth: unset role")
		}
		return s.defaultRole(), false, nil
	}

	roleOrder := []org.RoleType{RoleGrafanaAdmin, org.RoleAdmin, org.RoleEditor,
		org.RoleViewer, org.RoleNone}
	for _, role := range roleOrder {
		if found := hasRole(claims.Roles, role); found {
			if role == RoleGrafanaAdmin {
				return org.RoleAdmin, true, nil
			}

			return role, false, nil
		}
	}

	if s.roleAttributeStrict {
		return "", false, errRoleAttributeStrictViolation.Errorf("AzureAD OAuth: idP did not return a valid role %q", claims.Roles)
	}

	return s.defaultRole(), false, nil
}
```

### `extractGroups` (method, added) in azuread_oauth.go:293-349
Called by: UserInfo (azuread_oauth.go)
Calls: Client, groupsGraphAPIURL
```
func (s *SocialAzureAD) extractGroups(ctx context.Context, client *http.Client, claims *azureClaims, token *oauth2.Token) ([]string, error) {
	if !s.forceUseGraphAPI {
		s.log.Debug("Checking the claim for groups")
		if len(claims.Groups) > 0 {
			return claims.Groups, nil
		}

		if claims.ClaimNames.Groups == "" {
			return []string{}, nil
		}
	}

	// Fallback to the Graph API
	endpoint, errBuildGraphURI := s.groupsGraphAPIURL(claims, token)
	if errBuildGraphURI != nil {
		return nil, errBuildGraphURI
	}

	data, err := json.Marshal(&getAzureGroupRequest{SecurityEnabledOnly: false})
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, bytes.NewBuffer(data))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")

	res, err := client.Do(req)
	if err != nil {
		return nil, err
	}

	defer func() {
		if err := res.Body.Close(); err != nil {
			s.log.Warn("AzureAD OAuth: failed to close response body", "err", err)
		}
	}()

	if res.StatusCode != http.StatusOK {
		if res.StatusCode == http.StatusForbidden {
			s.log.Warn("AzureAD OAuth: Token need GroupMember.Read.All permission to fetch all group
... (truncated)
```

### `isGroupMember` (method, added) in social.go:443-457
Called by: UserInfo (azuread_oauth.go), UserInfo (gitlab_oauth.go), UserInfo (google_oauth.go)
```
func (s *SocialBase) isGroupMember(groups []string) bool {
	if len(s.allowedGroups) == 0 {
		return true
	}

	for _, allowedGroup := range s.allowedGroups {
		for _, group := range groups {
			if group == allowedGroup {
				return true
			}
		}
	}

	return false
}
```

### `Authenticate` (method, modified) in oauth.go:82-169
Calls: Exchange, UserInfo, Client
```
func (c *OAuth) Authenticate(ctx context.Context, r *authn.Request) (*authn.Identity, error) {
	r.SetMeta(authn.MetaKeyAuthModule, c.moduleName)
	// get hashed state stored in cookie
	stateCookie, err := r.HTTPRequest.Cookie(oauthStateCookieName)
	if err != nil {
		return nil, errOAuthMissingState.Errorf("missing state cookie")
	}

	if stateCookie.Value == "" {
		return nil, errOAuthMissingState.Errorf("missing state value in state cookie")
	}

	// get state returned by the idp and hash it
	stateQuery := hashOAuthState(r.HTTPRequest.URL.Query().Get(oauthStateQueryName), c.cfg.SecretKey, c.oauthCfg.ClientSecret)
	// compare the state returned by idp against the one we stored in cookie
	if stateQuery != stateCookie.Value {
		return nil, errOAuthInvalidState.Errorf("provided state did not match stored state")
	}

	var opts []oauth2.AuthCodeOption
	// if pkce is enabled for client validate we have the cookie and set it as url param
	if c.oauthCfg.UsePKCE {
		pkceCookie, err := r.HTTPRequest.Cookie(oauthPKCECookieName)
		if err != nil {
			return nil, errOAuthMissingPKCE.Errorf("no pkce cookie found: %w", err)
		}
		opts = append(opts, oauth2.SetAuthURLParam(codeVerifierParamName, pkceC
... (truncated)
```

### `validateIDTokenSignature` (method, added) in azuread_oauth.go:195-230
Called by: validateClaims (azuread_oauth.go)
Calls: Client, retrieveJWKSFromCache, retrieveSpecificJWKS
```
func (s *SocialAzureAD) validateIDTokenSignature(ctx context.Context, client *http.Client, parsedToken *jwt.JSONWebToken) (*azureClaims, error) {
	var claims azureClaims

	jwksFuncs := []func(ctx context.Context, client *http.Client, authURL string) (*keySetJWKS, time.Duration, error){
		s.retrieveJWKSFromCache, s.retrieveSpecificJWKS, s.retrieveGeneralJWKS,
	}

	keyID := parsedToken.Headers[0].KeyID

	for _, jwksFunc := range jwksFuncs {
		keyset, expiry, err := jwksFunc(ctx, client, s.Endpoint.AuthURL)
		if err != nil {
			return nil, fmt.Errorf("error retrieving jwks: %w", err)
		}
		var errClaims error
		keys := keyset.Key(keyID)
		for _, key := range keys {
			s.log.Debug("AzureAD OAuth: trying to parse token with key", "kid", key.KeyID)
			if errClaims = parsedToken.Claims(key, &claims); errClaims == nil {
				if expiry != 0 {
					s.log.Debug("AzureAD OAuth: caching key set", "kid", key.KeyID, "expiry", expiry)
					if err := s.cacheJWKS(ctx, keyset, expiry); err != nil {
						s.log.Warn("Failed to set key set in cache", "err", err)
					}
				}
				return &claims, nil
			} else {
				s.log.Warn("AzureAD OAuth: failed to parse token with key", "kid", key.KeyID, "err", errCl
... (truncated)
```

### `isAllowedTenant` (method, added) in azuread_oauth.go:396-408
Called by: validateClaims (azuread_oauth.go)
```
func (s *SocialAzureAD) isAllowedTenant(tenantID string) bool {
	if len(s.allowedOrganizations) == 0 {
		s.log.Warn("No allowed organizations specified, all tenants are allowed. Configure allowed_organizations to restrict access")
		return true
	}

	for _, t := range s.allowedOrganizations {
		if t == tenantID {
			return true
		}
	}
	return false
}
```

### `defaultRole` (method, added) in social.go:305-313
Called by: extractRoleAndAdmin (azuread_oauth.go), UserInfo (generic_oauth.go), extractRoleAndAdmin (social.go)
```
func (s *SocialBase) defaultRole() org.RoleType {
	if s.autoAssignOrgRole != "" {
		s.log.Debug("No role found, returning default.")
		return org.RoleType(s.autoAssignOrgRole)
	}

	// should never happen
	return org.RoleViewer
}
```

### `hasRole` (function, added) in azuread_oauth.go:270-278
Called by: extractRoleAndAdmin (azuread_oauth.go), TestMigrations (ac_test.go), TestManagedPermissionsMigration (managed_permission_migrator_test.go)
```
func hasRole(roles []string, role org.RoleType) bool {
	for _, item := range roles {
		if strings.EqualFold(item, string(role)) {
			return true
		}
	}

	return false
}
```

### `UserInfo` (method, added) in github_oauth.go:223-300
Calls: Client, httpGet, FetchTeamMemberships
```
func (s *SocialGithub) UserInfo(ctx context.Context, client *http.Client, token *oauth2.Token) (*BasicUserInfo, error) {
	var data struct {
		Id    int    `json:"id"`
		Login string `json:"login"`
		Email string `json:"email"`
		Name  string `json:"name"`
	}

	response, err := s.httpGet(ctx, client, s.apiUrl)
	if err != nil {
		return nil, fmt.Errorf("error getting user info: %s", err)
	}

	if err = json.Unmarshal(response.Body, &data); err != nil {
		return nil, fmt.Errorf("error unmarshalling user info: %s", err)
	}

	teamMemberships, err := s.FetchTeamMemberships(ctx, client)
	if err != nil {
		return nil, fmt.Errorf("error getting user teams: %s", err)
	}

	teams := convertToGroupList(teamMemberships)

	var role roletype.RoleType
	var isGrafanaAdmin *bool = nil

	if !s.skipOrgRoleSync {
		var grafanaAdmin bool
		role, grafanaAdmin, err = s.extractRoleAndAdmin(response.Body, teams)
		if err != nil {
			return nil, err
		}

		if s.allowAssignGrafanaAdmin {
			isGrafanaAdmin = &grafanaAdmin
		}
	}

	// we skip allowing assignment of GrafanaAdmin if skipOrgRoleSync is present
	if s.allowAssignGrafanaAdmin && s.skipOrgRoleSync {
		s.log.Debug("AllowAssignGrafanaAdmin and skipOrgRole
... (truncated)
```

### `extractFromAPI` (method, added) in gitlab_oauth.go:189-236
Called by: UserInfo (gitlab_oauth.go)
Calls: Client, httpGet, getGroups
```
func (s *SocialGitlab) extractFromAPI(ctx context.Context, client *http.Client, token *oauth2.Token) (*userData, error) {
	apiResp := &apiData{}
	response, err := s.httpGet(ctx, client, s.apiUrl+"/user")
	if err != nil {
		return nil, fmt.Errorf("Error getting user info: %w", err)
	}

	if err = json.Unmarshal(response.Body, &apiResp); err != nil {
		return nil, fmt.Errorf("error getting user info: %w", err)
	}

	// check confirmed_at exists and is not null
	if apiResp.ConfirmedAt == nil || *apiResp.ConfirmedAt == "" {
		return nil, fmt.Errorf("user %s's email is not confirmed", apiResp.Username)
	}

	if apiResp.State != "active" {
		return nil, fmt.Errorf("user %s is inactive", apiResp.Username)
	}

	idData := &userData{
		ID:     fmt.Sprintf("%d", apiResp.ID),
		Login:  apiResp.Username,
		Email:  apiResp.Email,
		Name:   apiResp.Name,
		Groups: s.getGroups(ctx, client),
	}

	if !s.skipOrgRoleSync {
		var grafanaAdmin bool
		role, grafanaAdmin, err := s.extractRoleAndAdmin(response.Body, idData.Groups)
		if err != nil {
			return nil, err
		}

		if s.allowAssignGrafanaAdmin {
			idData.IsGrafanaAdmin = &grafanaAdmin
		}

		idData.Role = role
	}

	if setting.Env == setting.Dev {
		
... (truncated)
```

## Hypotheses to verify
