class Team < ApplicationRecord
  has_many :memberships, dependent: :destroy
  has_many :invitations, dependent: :destroy
  has_many :members, through: :memberships, source: :user

  def admin?(user)
    memberships.exists?(user: user, role: %w[owner admin])
  end
end
