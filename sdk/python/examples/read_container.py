from offf_sdk import OfffContainer


def main() -> None:
    container = OfffContainer("../../tests/samples/4orensics.case2.offf")
    print(f"Container ID: {container.container_id}")
    print(f"Source hash valid: {container.verify_source_hash()}")
    print(f"Merkle root valid: {container.verify_merkle_root()}")

    first_kib = container.read_bytes(0, 1024)
    print(f"Read first KiB: {len(first_kib)} bytes")

    for i, event in enumerate(container.iter_provenance_events()):
        print(f"{event.event_id} {event.action} by {event.actor}")
        if i >= 4:
            break


if __name__ == "__main__":
    main()
