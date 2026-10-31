-- The other parts of a match's SourceTV (Flashy, October 2026: "make it fetch
-- the missing STV parts automatically"). A server splits its recording into
-- files of about eighteen minutes; demos.tf lists each as a demo of its own,
-- and a log is linked to one of them. These are the rest, found on demos.tf
-- by map and time, fetched along with the one the log is linked to.
CREATE TABLE stv_part (
    log_id      INTEGER NOT NULL,
    demos_tf_id INTEGER NOT NULL,
    PRIMARY KEY (log_id, demos_tf_id),
    FOREIGN KEY (log_id) REFERENCES match (log_id) ON DELETE CASCADE
) STRICT;
