rule find_bitcoin_address {
    meta:
        description = "Detects Bitcoin address patterns"
    strings:
        $btc2 = /bc1[ac-hj-np-z02-9]{6,87}/
    condition:
        $btc2
}

rule find_windows_credential_strings {
    meta:
        description = "Common credential-related strings"
    strings:
        $a = "password" nocase
        $b = "passwd" nocase
        $c = "credentials" nocase
    condition:
        any of them
}

rule find_office_document_magic {
    meta:
        description = "Microsoft Office Open XML magic bytes"
    strings:
        $zip = { 50 4B 03 04 }
        $cfb = { D0 CF 11 E0 A1 B1 1A E1 }
    condition:
        any of them
}