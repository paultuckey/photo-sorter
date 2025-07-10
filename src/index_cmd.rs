
// todo: WIP

fn paths() {
    // todo: specify as glob patterns or regex?
    //   how to relate photos/videos to corresponding metadata?
    //   how to relate albums to corresponding photos/videos?
    //   do we care about edits to originals? yes ideally we would have one md with multiple
    //   what do we do about extra files that are not in the metadata? (eg, stuff that is in other users takeout but not ours)

    // IC
    // albums
    let ic_root = "Albums/*.csv";
    let ic_root = "Memories/**/*.csv";

    // photos
    let ic_root = "Photos/*.[HEIC|MOV|JPG|jpeg|MOV]";
    let ic_root = "Recently Deleted/*.[HEIC|MOV|JPG|jpeg|MOV]";

    // index of files with meta
    let ic_root = "Photos/Photo Details*.csv"; // may end with -1 -2 etc if there are many files

    // G
    // people
    let ic_root = "Google Photos/, */*.[HEIC|JPG|MOV]";

    // albums
    let ic_root = "Google Photos/, */*metadata.json";

    // photos
    let ic_root = "Google Photos/Photos from */*.[HEIC|JPG|MOV]";
    let ic_root = "Google Photos/Photos from */*.suppl.json";
    let ic_root = "Google Photos/Photos from */*.supplemental-metadata.json";


}
