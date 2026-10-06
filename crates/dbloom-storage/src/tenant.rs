//! 多租户过滤封装（D8）。
//!
//! 普通用户所有查询自动附加 `owner_user_id = ?`；管理员可通过
//! `explicit_user_id` 显式跨用户（`docs/design/02-api.md` §1 租户过滤）。

/// 租户作用域：描述当前请求方对业务实体的可见范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantScope {
    /// 当前操作者用户 id。
    pub actor_user_id: i64,
    /// 是否管理员（admin 可显式指定跨用户）。
    pub is_admin: bool,
    /// 管理员显式指定的目标用户（`?user_id=`，None=全量）。
    pub explicit_user_id: Option<i64>,
}

impl TenantScope {
    pub fn actor(user_id: i64, is_admin: bool) -> Self {
        Self {
            actor_user_id: user_id,
            is_admin,
            explicit_user_id: None,
        }
    }

    /// 管理员显式指定跨用户视角。
    pub fn admin_explicit(user_id: i64, target: i64) -> Self {
        Self {
            actor_user_id: user_id,
            is_admin: true,
            explicit_user_id: Some(target),
        }
    }

    /// 是否为管理员且未限制到具体用户（可看全量）。
    pub fn can_see_all(&self) -> bool {
        self.is_admin && self.explicit_user_id.is_none()
    }

    /// 对一条 `owner_user_id` 记录是否可见。
    pub fn can_access_owner(&self, owner_user_id: i64) -> bool {
        if self.can_see_all() {
            return true;
        }
        if let Some(t) = self.explicit_user_id {
            return t == owner_user_id;
        }
        self.actor_user_id == owner_user_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_can_only_see_own() {
        let s = TenantScope::actor(1, false);
        assert!(s.can_access_owner(1));
        assert!(!s.can_access_owner(2));
        assert!(!s.can_see_all());
    }

    #[test]
    fn admin_sees_all() {
        let s = TenantScope::actor(9, true);
        assert!(s.can_see_all());
        assert!(s.can_access_owner(1));
        assert!(s.can_access_owner(2));
    }

    #[test]
    fn admin_explicit_sees_only_target() {
        let s = TenantScope::admin_explicit(9, 3);
        assert!(!s.can_see_all());
        assert!(s.can_access_owner(3));
        assert!(!s.can_access_owner(1));
    }
}
