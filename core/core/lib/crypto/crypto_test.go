package crypto

import (
	"bytes"
	"testing"
)

func TestGenerateKeyLength(t *testing.T) {
	key, err := GenerateKey()
	if err != nil {
		t.Fatal(err)
	}
	if len(key) != 32 {
		t.Fatalf("expected 32 bytes, got %d", len(key))
	}
}

func TestGenerateKeyIsRandom(t *testing.T) {
	k1, _ := GenerateKey()
	k2, _ := GenerateKey()
	if bytes.Equal(k1, k2) {
		t.Fatal("two generated keys are identical — entropy failure")
	}
}

func TestEncryptDecryptRoundTrip(t *testing.T) {
	key, _ := GenerateKey()
	plaintext := []byte("hello, whoisthat!")

	ciphertext, err := Encrypt(plaintext, key)
	if err != nil {
		t.Fatal(err)
	}

	decrypted, err := Decrypt(ciphertext, key)
	if err != nil {
		t.Fatal(err)
	}

	if !bytes.Equal(plaintext, decrypted) {
		t.Fatalf("round-trip mismatch: got %q, want %q", decrypted, plaintext)
	}
}

func TestEncryptProducesNonDeterministicOutput(t *testing.T) {
	key, _ := GenerateKey()
	plaintext := []byte("same input")

	c1, _ := Encrypt(plaintext, key)
	c2, _ := Encrypt(plaintext, key)
	if bytes.Equal(c1, c2) {
		t.Fatal("two encryptions of the same plaintext are identical — nonce not random")
	}
}

func TestDecryptShortCiphertextError(t *testing.T) {
	key, _ := GenerateKey()
	_, err := Decrypt([]byte("short"), key)
	if err == nil {
		t.Fatal("expected error for short ciphertext")
	}
}

func TestDecryptWrongKeyError(t *testing.T) {
	key1, _ := GenerateKey()
	key2, _ := GenerateKey()
	ciphertext, _ := Encrypt([]byte("secret"), key1)
	_, err := Decrypt(ciphertext, key2)
	if err == nil {
		t.Fatal("expected error when decrypting with wrong key")
	}
}

func TestEncryptToBase64DecryptBase64RoundTrip(t *testing.T) {
	key, _ := GenerateKey()
	plaintext := []byte(`{"msg":"test","data":{}}`)

	encoded, err := EncryptToBase64(plaintext, key)
	if err != nil {
		t.Fatal(err)
	}

	decoded, err := DecryptBase64(encoded, key)
	if err != nil {
		t.Fatal(err)
	}

	if !bytes.Equal(plaintext, decoded) {
		t.Fatalf("base64 round-trip mismatch: got %q", decoded)
	}
}

func TestDecryptBase64InvalidBase64(t *testing.T) {
	key, _ := GenerateKey()
	_, err := DecryptBase64("not!!valid==base64", key)
	if err == nil {
		t.Fatal("expected error for invalid base64 input")
	}
}

func TestEncryptDecryptEmptyPlaintext(t *testing.T) {
	key, _ := GenerateKey()
	ciphertext, err := Encrypt([]byte{}, key)
	if err != nil {
		t.Fatal(err)
	}
	decrypted, err := Decrypt(ciphertext, key)
	if err != nil {
		t.Fatal(err)
	}
	if len(decrypted) != 0 {
		t.Fatalf("expected empty decrypted, got %d bytes", len(decrypted))
	}
}
