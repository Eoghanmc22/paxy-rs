pub mod handling;

pub enum TransformationResult {
    Unchanged,
    Modified,
    Canceled
}

impl TransformationResult {
    pub(crate) fn combine(&mut self, other: TransformationResult) -> bool {
        match self {
            TransformationResult::Unchanged => {
                match other {
                    TransformationResult::Modified => {
                        *self = TransformationResult::Modified;
                    }
                    TransformationResult::Canceled => {
                        *self = TransformationResult::Canceled;
                        return true;
                    }
                    _ => {}
                }
            }
            TransformationResult::Modified => {
                match other {
                    TransformationResult::Canceled => {
                        *self = TransformationResult::Canceled;
                        return true;
                    }
                    _ => {}
                }
            }
            TransformationResult::Canceled => {
                return true;
            }
        }
        false
    }
}