export function base64ToBlob(base64Data: string): Blob {
  // Step 1: Split the Data URL to get the Base64-encoded data
  const parts: string[] = base64Data.split(';base64,');
  const contentType: string = parts[0].split(':')[1]; // e.g., "image/gif"
  const rawBase64: string = parts[1];

  // Step 2: Decode the Base64 string to a binary string
  const binaryStr: string = window.atob(rawBase64);

  // Step 3: Convert binary string to a typed array
  const len: number = binaryStr.length;
  const arrayBuffer: Uint8Array = new Uint8Array(len);
  for (let i = 0; i < len; i++) {
    arrayBuffer[i] = binaryStr.charCodeAt(i);
  }

  // Step 4: Create a Blob from the typed array
  const blob: Blob = new Blob([arrayBuffer as any], { type: contentType });

  return blob;
}
