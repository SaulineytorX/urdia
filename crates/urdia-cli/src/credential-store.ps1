# Fixed Urdia generic credential bridge. Key material enters/leaves only stdin/stdout.
$ErrorActionPreference = 'Stop'
try {
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class UrdiaCredential {
 [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
 public struct Credential {
  public UInt32 Flags; public UInt32 Type;
  public string TargetName; public string Comment;
  public System.Runtime.InteropServices.ComTypes.FILETIME LastWritten;
  public UInt32 CredentialBlobSize; public IntPtr CredentialBlob;
  public UInt32 Persist; public UInt32 AttributeCount; public IntPtr Attributes;
  public string TargetAlias; public string UserName;
 }
 [DllImport("advapi32.dll", EntryPoint="CredWriteW", CharSet=CharSet.Unicode, SetLastError=true)]
 public static extern bool Write(ref Credential credential, UInt32 flags);
 [DllImport("advapi32.dll", EntryPoint="CredReadW", CharSet=CharSet.Unicode, SetLastError=true)]
 public static extern bool Read(string target, UInt32 type, UInt32 flags, out IntPtr credential);
 [DllImport("advapi32.dll", EntryPoint="CredDeleteW", CharSet=CharSet.Unicode, SetLastError=true)]
 public static extern bool Delete(string target, UInt32 type, UInt32 flags);
 [DllImport("advapi32.dll", EntryPoint="CredFree")]
 public static extern void Free(IntPtr credential);
}
'@
  $request = [Console]::In.ReadToEnd() | ConvertFrom-Json
  $target = 'Urdia/Jev/v1'
  if ($request.action -eq 'read') {
    $pointer = [IntPtr]::Zero
    if (-not [UrdiaCredential]::Read($target, 1, 0, [ref]$pointer)) {
      if ([Runtime.InteropServices.Marshal]::GetLastWin32Error() -eq 1168) { exit 2 }
      throw 'vault unavailable'
    }
    try {
      $credential = [Runtime.InteropServices.Marshal]::PtrToStructure($pointer, [type][UrdiaCredential+Credential])
      if ($credential.CredentialBlobSize -gt 4096) { throw 'invalid size' }
      $bytes = New-Object byte[] $credential.CredentialBlobSize
      [Runtime.InteropServices.Marshal]::Copy($credential.CredentialBlob, $bytes, 0, $bytes.Length)
      [Console]::Out.Write([Text.Encoding]::UTF8.GetString($bytes))
    } finally { [UrdiaCredential]::Free($pointer) }
  } elseif ($request.action -eq 'write') {
    $bytes = [Text.Encoding]::UTF8.GetBytes([string]$request.key)
    if ($bytes.Length -eq 0 -or $bytes.Length -gt 2560) { throw 'invalid size' }
    $pointer = [Runtime.InteropServices.Marshal]::AllocCoTaskMem($bytes.Length)
    try {
      [Runtime.InteropServices.Marshal]::Copy($bytes, 0, $pointer, $bytes.Length)
      $credential = New-Object UrdiaCredential+Credential
      $credential.Type = 1; $credential.TargetName = $target
      $credential.UserName = 'Urdia'; $credential.Persist = 2
      $credential.CredentialBlobSize = $bytes.Length; $credential.CredentialBlob = $pointer
      if (-not [UrdiaCredential]::Write([ref]$credential, 0)) { throw 'vault unavailable' }
    } finally { [Runtime.InteropServices.Marshal]::FreeCoTaskMem($pointer) }
  } elseif ($request.action -eq 'delete') {
    if (-not [UrdiaCredential]::Delete($target, 1, 0) -and
        [Runtime.InteropServices.Marshal]::GetLastWin32Error() -ne 1168) { throw 'vault unavailable' }
  } else { throw 'invalid action' }
  exit 0
} catch { [Console]::Error.WriteLine('Urdia credential operation failed'); exit 1 }
