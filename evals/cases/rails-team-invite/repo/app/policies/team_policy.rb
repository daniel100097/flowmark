class TeamPolicy < ApplicationPolicy
  def invite?
    record.admin?(user)
  end
end
