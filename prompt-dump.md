# System Prompt

You are a precision code reviewer. Find only high-confidence, concrete correctness bugs.

Review protocol — follow this order strictly:

PHASE 1 (no tools): Read all provided code. For EACH entity with a "Contract:" block, verify the implementation satisfies the contract. Check:
- Name/contract mismatch: if a method is named getX or documented "@return X", does it ACTUALLY return X? Flag if it returns a generic/default value instead.
- Fluent/builder misuse: are return values from fluent/builder APIs captured? If discarded, the operation is a no-op.
- Dead code: are any computed results unused or overwritten?
- Guard removal: were safety checks (assertions, null guards) removed?

PHASE 2 (tools): Use read/grep ONLY to confirm or refute your Phase 1 suspicions. Do not explore broadly.

Do NOT report: style, naming, missing tests, documentation, suggestions, or issues in deleted-only code.

Respond with ONLY a JSON object:
{"issues": [{"issue": "description naming exact function/variable and the concrete bug", "evidence": "exact code snippet", "severity": "critical|high|medium", "file": "path/to/file"}]}

Return {"issues": []} if no bugs.

---

# User Prompt

# PR: Code review
Top 30 entities across 12 files (from 79 total)

## Detector Findings (8)
- **[MEDIUM] variable-near-miss** `WildFlyElytronProvider` crypto/elytron/src/main/java/org/keycloak/crypto/elytron/WildFlyElytronProvider.java:88
  Identifier changed from `getAesGcmCipher` to similar `getAesCbcCipher` — possible wrong-variable usage
  `public Cipher getAesCbcCipher() throws NoSuchAlgorithmException, NoSuchPaddingException {` | risk=High(0.64) deps=0
- **[MEDIUM] variable-near-miss** `FIPS1402Provider` crypto/fips1402/src/main/java/org/keycloak/crypto/fips/FIPS1402Provider.java:123
  Identifier changed from `getAesGcmCipher` to similar `getAesCbcCipher` — possible wrong-variable usage
  `public Cipher getAesCbcCipher() throws NoSuchAlgorithmException, NoSuchProviderException, NoSuchPaddingException {` | risk=Critical(0.95) deps=3
- **[MEDIUM] type-change-propagation** `AuthzClient` authz/client/src/main/java/org/keycloak/authorization/client/AuthzClient.java:47
  Type `AuthzClient` was modified but 73 dependent(s) were not updated in this diff: AbstractResourceServerTest, getAuthzClient, AuthorizationAPITest, testAccessTokenWithUmaAuthorization, testResponseMode and 68 more
  `Unchanged dependents: AbstractResourceServerTest, getAuthzClient, AuthorizationAPITest, testAccessTokenWithUmaAuthorization, testResponseMode and 68 more` | risk=Critical(1.00) deps=76
- **[MEDIUM] type-change-propagation** `CryptoIntegration` common/src/main/java/org/keycloak/common/crypto/CryptoIntegration.java:22
  Type `CryptoIntegration` was modified but 170 dependent(s) were not updated in this diff: Logger, BouncyIntegration, loadProvider, CertificateUtils, generateV3Certificate and 165 more
  `Unchanged dependents: Logger, BouncyIntegration, loadProvider, CertificateUtils, generateV3Certificate and 165 more` | risk=Low(0.15) deps=172
- **[MEDIUM] type-change-propagation** `FIPS1402Provider` crypto/fips1402/src/main/java/org/keycloak/crypto/fips/FIPS1402Provider.java:73
  Type `FIPS1402Provider` was modified but 3 dependent(s) were not updated in this diff: FipsMode, Logger, Fips1402StrictCryptoProvider
  `Unchanged dependents: FipsMode, Logger, Fips1402StrictCryptoProvider` | risk=Critical(0.95) deps=3
- **[MEDIUM] type-change-propagation** `DefaultCryptoProvider` crypto/default/src/main/java/org/keycloak/crypto/def/DefaultCryptoProvider.java:46
  Type `DefaultCryptoProvider` was modified but 2 dependent(s) were not updated in this diff: FipsMode, Logger
  `Unchanged dependents: FipsMode, Logger` | risk=Critical(0.87) deps=2
- **[HIGH] removed-guard** `CryptoIntegration` common/src/main/java/org/keycloak/common/crypto/CryptoIntegration.java:1
  Guard/assertion removed: `throw new IllegalStateException("Multiple crypto providers loaded with the class` — safety check may be lost
  `throw new IllegalStateException("Multiple crypto providers loaded with the classLoader: " + classLoader +` | risk=Low(0.15) deps=172
- **[HIGH] removed-guard** `detectProvider` common/src/main/java/org/keycloak/common/crypto/CryptoIntegration.java:1
  Guard/assertion removed: `throw new IllegalStateException("Multiple crypto providers loaded with the class` — safety check may be lost
  `throw new IllegalStateException("Multiple crypto providers loaded with the classLoader: " + classLoader +` | risk=Critical(1.58) deps=2

## common/src/main/java/org/keycloak/common/crypto/CryptoIntegration.java (Critical, 2 entities)
### `detectProvider` (method, modified) :55-73
risk=Critical(1.58) blast=10000 deps=2 | callers: CryptoIntegration, init
```
private static CryptoProvider detectProvider(ClassLoader classLoader) {
        List<CryptoProvider> foundProviders = StreamSupport.stream(ServiceLoader.load(CryptoProvider.class, classLoader).spliterator(), false)
                .sorted(Comparator.comparingInt(CryptoProvider::order).reversed())
                .collect(Collectors.toList());

        if (foundProviders.isEmpty()) {
            throw new IllegalStateException("Not able to load any cryptoProvider with the classLoader: " + classLoader);
        } else {
            logger.debugf("Detected crypto provider: %s", foundProviders.get
... (456 more chars)
```

### `CryptoIntegration` (class, modified) :22-100
risk=Low(0.15) blast=10000 deps=172 **PUBLIC** | callers: AuthzClient, create, Logger, BouncyIntegration +168
```
public class CryptoIntegration {

    protected static final Logger logger = Logger.getLogger(CryptoIntegration.class);

    private static final Object lock = new Object();
    private static volatile CryptoProvider cryptoProvider;

    public static void init(ClassLoader classLoader) {
        if (cryptoProvider == null) {
            synchronized (lock) {
                if (cryptoProvider == null) {
                    cryptoProvider = detectProvider(classLoader);
                    logger.debugv("java security provider: {0}", BouncyIntegration.PROVIDER);

                }
            }

... (3134 more chars)
```

## authz/client/src/main/java/org/keycloak/authorization/client/AuthzClient.java (Critical, 1 entities)
### `AuthzClient` (class, modified) :47-280
risk=Critical(1.00) blast=140 deps=76 **PUBLIC** | callers: create, create, create, AbstractResourceServerTest +72
```
public class AuthzClient {

    private final Http http;
    private TokenCallable patSupplier;

    /**
     * <p>Creates a new instance.
     *
     * <p>This method expects a <code>keycloak.json</code> in the classpath, otherwise an exception will be thrown.
     *
     * @return a new instance
     * @throws RuntimeException in case there is no <code>keycloak.json</code> file in the classpath or the file could not be parsed
     */
    public static AuthzClient create() throws RuntimeException {
        InputStream configStream = Thread.currentThread().getContextClassLoader().getResourceAs
... (8190 more chars)
```

## authz/client/src/main/java/org/keycloak/authorization/client/util/crypto/AuthzClientCryptoProvider.java (Critical, 15 entities)
### `getSignature` (method, added) :217-220
risk=Critical(1.00) blast=10000 deps=65 **PUBLIC** | callers: AuthzClientCryptoProvider, TokenVerifier, verifySignature, SdJws +61
**Contract:**
```
Signature getSignature(String sigAlgName) throws NoSuchAlgorithmException, NoSuchProviderException;
```
```
@Override
    public Signature getSignature(String sigAlgName) throws NoSuchAlgorithmException, NoSuchProviderException {
        throw new UnsupportedOperationException("Not supported yet.");
    }
```

### `getKeyPairGen` (method, added) :172-175
risk=Low(0.14) blast=10000 deps=27 **PUBLIC** | callers: AuthzClientCryptoProvider, KeyUtils, generateRsaKeyPair, JWKTest +23
**Contract:**
```
KeyPairGenerator getKeyPairGen(String algorithm) throws NoSuchAlgorithmException, NoSuchProviderException;
```
```
@Override
    public KeyPairGenerator getKeyPairGen(String algorithm) throws NoSuchAlgorithmException, NoSuchProviderException {
        throw new UnsupportedOperationException("Not supported yet.");
    }
```

### `getKeyStore` (method, added) :197-200
risk=Low(0.14) blast=10000 deps=13 **PUBLIC** | callers: AuthzClientCryptoProvider, KeystoreUtil, loadKeyPairFromKeystore, ClientAttributeCertificateResource +9
**Contract:**
```
KeyStore getKeyStore(KeystoreFormat format) throws KeyStoreException, NoSuchProviderException;
```
```
@Override
    public KeyStore getKeyStore(KeystoreUtil.KeystoreFormat format) throws KeyStoreException, NoSuchProviderException {
        return KeyStore.getInstance(format.name());
    }
```

### `concatenatedRSToASN1DER` (method, added) :102-122
risk=Low(0.14) blast=10000 deps=10 **PUBLIC** | callers: AuthzClientCryptoProvider, getEcdsaCryptoProvider, ECDSAAlgorithmTest, test +6
⚠️ **Review cue:** Method name contains "concatenated, rsto, asn1der" but implementation body does not reference them. Verify the implementation matches the contract.
```
@Override
            public byte[] concatenatedRSToASN1DER(byte[] signature, int signLength) throws IOException {
                int len = signLength / 2;
                int arraySize = len + 1;

                byte[] r = new byte[arraySize];
                byte[] s = new byte[arraySize];
                System.arraycopy(signature, 0, r, 1, len);
                System.arraycopy(signature, len, s, 1, len);
                BigInteger rBigInteger = new BigInteger(r);
                BigInteger sBigInteger = new BigInteger(s);

                ASN1Encoder.create().write(rBigInteger);
       
... (332 more chars)
```

### `getKeyFactory` (method, added) :177-180
risk=Low(0.13) blast=10000 deps=16 **PUBLIC** | callers: AuthzClientCryptoProvider, DerUtils, decodePrivateKey, decodePublicKey +12
**Contract:**
```
KeyFactory getKeyFactory(String algorithm) throws NoSuchAlgorithmException, NoSuchProviderException;
```
```
@Override
    public KeyFactory getKeyFactory(String algorithm) throws NoSuchAlgorithmException, NoSuchProviderException {
        throw new UnsupportedOperationException("Not supported yet.");
    }
```

### `getEcdsaCryptoProvider` (method, added) :99-165
risk=Low(0.13) blast=10000 deps=6 **PUBLIC** | callers: AuthzClientCryptoProvider, ECDSAAlgorithmTest, test, ECDSAAlgorithm +2
**Contract:**
```
public ECDSACryptoProvider getEcdsaCryptoProvider();
```
```
@Override
    public ECDSACryptoProvider getEcdsaCryptoProvider() {
        return new ECDSACryptoProvider() {
            @Override
            public byte[] concatenatedRSToASN1DER(byte[] signature, int signLength) throws IOException {
                int len = signLength / 2;
                int arraySize = len + 1;

                byte[] r = new byte[arraySize];
                byte[] s = new byte[arraySize];
                System.arraycopy(signature, 0, r, 1, len);
                System.arraycopy(signature, len, s, 1, len);
                BigInteger rBigInteger = new BigInteger(r);
  
... (2324 more chars)
```

### `getAlgorithmProvider` (method, added) :74-77
risk=Low(0.13) blast=10000 deps=14 **PUBLIC** | callers: AuthzClientCryptoProvider, JWERegistry, getAlgProvider, JWETest +10
**Contract:**
```
/**
* Get some algorithm provider implementation. Returned implementation can be dependent according to if we have
* non-fips bouncycastle or fips bouncycastle on the classpath.
*
* @param clazz Returned class.
* @param algorithm Type of the algorithm, which we want to return
* @return
*/
<T> T getAlgorithmProvider(Class<T> clazz, String algorithm);
```
```
@Override
    public <T> T getAlgorithmProvider(Class<T> clazz, String algorithm) {
        throw new UnsupportedOperationException("Not supported yet.");
    }
```

### `getPemUtils` (method, added) :84-87
risk=Low(0.13) blast=10000 deps=14 **PUBLIC** | callers: AuthzClientCryptoProvider, PemUtils, decodeCertificate, decodePublicKey +10
**Contract:**
```
/**
* Get PEMUtils implementation. Returned implementation can be dependent according to if we have
* non-fips bouncycastle or fips bouncycastle on the classpath.
*
* @return
*/
PemUtilsProvider getPemUtils();
```
```
@Override
    public PemUtilsProvider getPemUtils() {
        throw new UnsupportedOperationException("Not supported yet.");
    }
```

### `getCertificateUtils` (method, added) :79-82
risk=Low(0.13) blast=10000 deps=13 **PUBLIC** | callers: AuthzClientCryptoProvider, CertificateUtils, generateV3Certificate, generateV1SelfSignedCertificate +9
**Contract:**
```
/**
* Get CertificateUtils implementation. Returned implementation can be dependent according to if we have
* non-fips bouncycastle or fips bouncycastle on the classpath.
*
* @return
*/
CertificateUtilsProvider getCertificateUtils();
```
```
@Override
    public CertificateUtilsProvider getCertificateUtils() {
        throw new UnsupportedOperationException("Not supported yet.");
    }
```

### `asn1derToConcatenatedRS` (method, added) :124-144
risk=Low(0.13) blast=10000 deps=6 **PUBLIC** | callers: AuthzClientCryptoProvider, getEcdsaCryptoProvider, ECDSAAlgorithmTest, test +2
⚠️ **Review cue:** Method name contains "asn1der" but implementation body does not reference it. Verify the implementation matches the contract.
```
@Override
            public byte[] asn1derToConcatenatedRS(byte[] derEncodedSignatureValue, int signLength) throws IOException {
                int len = signLength / 2;

                List<byte[]> seq = ASN1Decoder.create(derEncodedSignatureValue).readSequence();
                if (seq.size() != 2) {
                    throw new IOException("Invalid sequence with size different to 2");
                }

                BigInteger rBigInteger = ASN1Decoder.create(seq.get(0)).readInteger();
                BigInteger sBigInteger = ASN1Decoder.create(seq.get(1)).readInteger();

          
... (406 more chars)
```

### `getIdentityExtractorProvider` (method, added) :94-97
risk=Low(0.13) blast=10000 deps=10 **PUBLIC** | callers: AuthzClientCryptoProvider, CertificateIdentityExtractorTest, testExtractsCertInPemFormat, testExtractsCertInSubjectDNFormat +6
**Contract:**
```
public UserIdentityExtractorProvider getIdentityExtractorProvider();
```
```
@Override
    public UserIdentityExtractorProvider getIdentityExtractorProvider() {
        throw new UnsupportedOperationException("Not supported yet.");
    }
```

### `getBouncyCastleProvider` (method, added) :60-67
risk=Low(0.12) blast=10000 deps=3 **PUBLIC** | callers: AuthzClientCryptoProvider, BouncyIntegration, loadProvider
**Contract:**
```
/**
* @return BouncyCastle security provider. Can be either non-FIPS or FIPS based provider
*/
Provider getBouncyCastleProvider();
```
⚠️ **Review cue:** Method name contains "bouncy, castle" but implementation body does not reference them. Verify the implementation matches the contract.
```
@Override
    public Provider getBouncyCastleProvider() {
        try {
            return KeyStore.getInstance(KeyStore.getDefaultType()).getProvider();
        } catch (KeyStoreException e) {
            throw new IllegalStateException(e);
        }
    }
```

### `getCertPathBuilder` (method, added) :212-215
risk=Low(0.11) blast=10000 deps=5 **PUBLIC** | callers: AuthzClientCryptoProvider, CertificateValidator, verifyCertificateTrust, NginxProxySslClientCertificateLookup +1
**Contract:**
```
CertPathBuilder getCertPathBuilder() throws NoSuchAlgorithmException, NoSuchProviderException;
```
```
@Override
    public CertPathBuilder getCertPathBuilder() throws NoSuchAlgorithmException, NoSuchProviderException {
        throw new UnsupportedOperationException("Not supported yet.");
    }
```

### `getCertStore` (method, added) :207-210
risk=Low(0.11) blast=10000 deps=5 **PUBLIC** | callers: AuthzClientCryptoProvider, CertificateValidator, verifyCertificateTrust, NginxProxySslClientCertificateLookup +1
**Contract:**
```
CertStore getCertStore(CollectionCertStoreParameters collectionCertStoreParameters) throws InvalidAlgorithmParameterException, NoSuchAlgorithmException, NoSuchProviderException;
```
```
@Override
    public CertStore getCertStore(CollectionCertStoreParameters collectionCertStoreParameters) throws InvalidAlgorithmParameterException, NoSuchAlgorithmException, NoSuchProviderException {
        throw new UnsupportedOperationException("Not supported yet.");
    }
```

### `getAesCbcCipher` (method, added) :182-185
risk=Low(0.11) blast=10000 deps=4 **PUBLIC** | callers: AuthzClientCryptoProvider, AesCbcHmacShaEncryptionProvider, encryptBytes, decryptBytes
**Contract:**
```
Cipher getAesCbcCipher() throws NoSuchAlgorithmException, NoSuchProviderException, NoSuchPaddingException;
```
```
@Override
    public Cipher getAesCbcCipher() throws NoSuchAlgorithmException, NoSuchProviderException, NoSuchPaddingException {
        throw new UnsupportedOperationException("Not supported yet.");
    }
```

## crypto/fips1402/src/main/java/org/keycloak/crypto/fips/FIPS1402Provider.java (Critical, 1 entities)
### `FIPS1402Provider` (class, modified) :73-391
risk=Critical(0.95) blast=10000 deps=3 **PUBLIC** | callers: FipsMode, Logger, Fips1402StrictCryptoProvider
```
public class FIPS1402Provider implements CryptoProvider {

    private static final Logger log = Logger.getLogger(FIPS1402Provider.class);

    private final BouncyCastleFipsProvider bcFipsProvider;
    private final Map<String, Object> providers = new ConcurrentHashMap<>();

    public FIPS1402Provider() {
        // Case when BCFIPS provider already registered in Java security file
        BouncyCastleFipsProvider existingBcFipsProvider = (BouncyCastleFipsProvider) Security.getProvider(CryptoConstants.BCFIPS_PROVIDER_ID);
        this.bcFipsProvider = existingBcFipsProvider == null ? new Bou
... (14236 more chars)
```

## crypto/default/src/main/java/org/keycloak/crypto/def/DefaultCryptoProvider.java (Critical, 1 entities)
### `DefaultCryptoProvider` (class, modified) :46-193
risk=Critical(0.87) blast=10000 deps=2 **PUBLIC** | callers: FipsMode, Logger
```
public class DefaultCryptoProvider implements CryptoProvider {

    private static final Logger log = Logger.getLogger(DefaultCryptoProvider.class);

    private final Provider bcProvider;

    private Map<String, Object> providers = new ConcurrentHashMap<>();

    public DefaultCryptoProvider() {
        // Make sure to instantiate this only once due it is expensive. And skip registration if already available in Java security providers (EG. due explicitly configured in java security file)
        Provider existingBc = Security.getProvider(CryptoConstants.BC_PROVIDER_ID);
        this.bcProvid
... (5147 more chars)
```

## authz/client/src/main/java/org/keycloak/authorization/client/util/crypto/ASN1Decoder.java (Critical, 3 entities)
### `readInteger` (method, added) :65-74
risk=Critical(0.82) blast=10000 deps=4 **PUBLIC** | callers: ASN1Decoder, AuthzClientCryptoProvider, getEcdsaCryptoProvider, asn1derToConcatenatedRS
```
public BigInteger readInteger() throws IOException {
        int tag = readTag();
        int tagNo = readTagNumber(tag);
        if (tagNo != ASN1Encoder.INTEGER) {
            throw new IOException("Invalid Integer tag " + tagNo);
        }
        int length = readLength();
        byte[] bytes = read(length);
        return new BigInteger(bytes);
    }
```

### `readSequence` (method, added) :49-63
risk=Low(0.12) blast=10000 deps=4 **PUBLIC** | callers: ASN1Decoder, AuthzClientCryptoProvider, getEcdsaCryptoProvider, asn1derToConcatenatedRS
```
public List<byte[]> readSequence() throws IOException {
        int tag = readTag();
        int tagNo = readTagNumber(tag);
        if (tagNo != ASN1Encoder.SEQUENCE) {
            throw new IOException("Invalid Sequence tag " + tagNo);
        }
        int length = readLength();
        List<byte[]> result = new ArrayList<>();
        while (length > 0) {
            byte[] bytes = readNext();
            result.add(bytes);
            length = length - bytes.length;
        }
        return result;
    }
```

### `ASN1Decoder` (class, added) :33-202
risk=Low(0.11) blast=10000 deps=4 | callers: create, AuthzClientCryptoProvider, getEcdsaCryptoProvider, asn1derToConcatenatedRS
```
class ASN1Decoder {

    private final ByteArrayInputStream is;
    private final int limit;
    private int count;

    ASN1Decoder(byte[] bytes) {
        is = new ByteArrayInputStream(bytes);
        count = 0;
        limit = bytes.length;
    }

    public static ASN1Decoder create(byte[] bytes) {
        return new ASN1Decoder(bytes);
    }

    public List<byte[]> readSequence() throws IOException {
        int tag = readTag();
        int tagNo = readTagNumber(tag);
        if (tagNo != ASN1Encoder.SEQUENCE) {
            throw new IOException("Invalid Sequence tag " + tagNo);
        
... (3970 more chars)
```

## authz/client/src/main/java/org/keycloak/authorization/client/util/crypto/ASN1Encoder.java (Critical, 2 entities)
### `ASN1Encoder` (class, added) :30-100
risk=Critical(0.80) blast=10000 deps=10 | callers: ASN1Decoder, readSequence, readInteger, create +6
```
class ASN1Encoder {

    static final int INTEGER = 0x02;
    static final int SEQUENCE = 0x10;
    static final int CONSTRUCTED = 0x20;

    private final ByteArrayOutputStream os;

    private ASN1Encoder() {
        this.os = new ByteArrayOutputStream();
    }

    static public ASN1Encoder create() {
        return new ASN1Encoder();
    }

    public ASN1Encoder write(BigInteger value) throws IOException {
        writeEncoded(INTEGER, value.toByteArray());
        return this;
    }

    public ASN1Encoder writeDerSeq(ASN1Encoder... objects) throws IOException {
        writeEncoded(CONS
... (1168 more chars)
```

### `writeDerSeq` (method, added) :51-54
risk=Low(0.11) blast=10000 deps=4 **PUBLIC** | callers: ASN1Encoder, AuthzClientCryptoProvider, getEcdsaCryptoProvider, concatenatedRSToASN1DER
```
public ASN1Encoder writeDerSeq(ASN1Encoder... objects) throws IOException {
        writeEncoded(CONSTRUCTED | SEQUENCE, concatenate(objects));
        return this;
    }
```

## crypto/elytron/src/main/java/org/keycloak/crypto/elytron/WildFlyElytronProvider.java (High, 1 entities)
### `WildFlyElytronProvider` (class, modified) :55-208
risk=High(0.64) blast=0 deps=0 **PUBLIC**
```
public class WildFlyElytronProvider implements CryptoProvider {

    private Map<String, Object> providers = new ConcurrentHashMap<>();

    public WildFlyElytronProvider() {
        providers.put(CryptoConstants.A128KW, new AesKeyWrapAlgorithmProvider());
        providers.put(CryptoConstants.RSA1_5, new ElytronRsaKeyEncryptionJWEAlgorithmProvider("RSA/ECB/PKCS1Padding"));
        providers.put(CryptoConstants.RSA_OAEP, new ElytronRsaKeyEncryptionJWEAlgorithmProvider("RSA/ECB/OAEPWithSHA-1AndMGF1Padding"));
        providers.put(CryptoConstants.RSA_OAEP_256, new ElytronRsaKeyEncryption256JWEA
... (4989 more chars)
```

## common/src/main/java/org/keycloak/common/crypto/CryptoProvider.java (Medium, 1 entities)
### `CryptoProvider` (interface, modified) :32-140
risk=Medium(0.49) blast=0 deps=0 **PUBLIC**
```
public interface CryptoProvider {

    /**
     * @return BouncyCastle security provider. Can be either non-FIPS or FIPS based provider
     */
    Provider getBouncyCastleProvider();

    /**
     * Order of this provider. This allows to specify which CryptoProvider will have preference in case that more of them are on the classpath.
     *
     * The higher number has preference over the lower number
     */
    int order();

    /**
     * Get some algorithm provider implementation. Returned implementation can be dependent according to if we have
     * non-fips bouncycastle or fips bouncyc
... (3359 more chars)
```

## authz/client/src/test/java/org/keycloak/authorization/client/test/ECDSAAlgorithmTest.java (Medium, 1 entities)
### `ECDSAAlgorithmTest` (class, added) :37-73
risk=Medium(0.39) blast=0 deps=0 **PUBLIC**
```
public class ECDSAAlgorithmTest {

    private final KeyPair keyPair;

    public ECDSAAlgorithmTest() throws Exception {
        keyPair = KeyPairGenerator.getInstance("EC").genKeyPair();
    }


    private void test(ECDSAAlgorithm algorithm) throws Exception {
        AuthzClientCryptoProvider prov = new AuthzClientCryptoProvider();
        byte[] data = "Something to sign".getBytes(StandardCharsets.UTF_8);
        Signature signature = Signature.getInstance(JavaAlgorithm.getJavaAlgorithm(algorithm.name()));
        signature.initSign(keyPair.getPrivate());
        signature.update(data);
 
... (765 more chars)
```

## authz/client/pom.xml (Medium, 1 entities)
### `artifactId` (element, modified) :0-0
risk=Medium(0.35) blast=0 deps=0
```
<artifactId>hamcrest</artifactId>
```

## authz/client/src/main/resources/META-INF/services/org.keycloak.common.crypto.CryptoProvider (Medium, 1 entities)
### `lines 1-20` (chunk, added) :0-0
risk=Medium(0.32) blast=0 deps=0
```
#
# Copyright 2024 Red Hat, Inc. and/or its affiliates
#  and other contributors as indicated by the @author tags.
#
#  Licensed under the Apache License, Version 2.0 (the "License");
#  you may not use this file except in compliance with the License.
#  You may obtain a copy of the License at
#
#  http://www.apache.org/licenses/LICENSE-2.0
#
#  Unless required by applicable law or agreed to in writing, software
#  distributed under the License is distributed on an "AS IS" BASIS,
#  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#
#  See the License for the specific language governing permissions and
#  limitations under the License.
#
#

org.keycloak.authorization.client.util.crypto.AuthzClientCryptoProvider
```
